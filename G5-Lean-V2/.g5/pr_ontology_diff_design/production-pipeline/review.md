# Production pipeline implementation review

## Consolidated IAM audit — 2026-10-26

Initial findings before repairs, on rebased #735 `50a10a59f` / main `4edb068a5`:

| Issue | Audited finding and proposed repair | Initial evidence / limit |
| --- | --- | --- |
| #739 | Hosts use the node/default Compute SA; add dedicated host identity and exact consumers | VM insertion, including zone fallback, attaches default; GKE still needs its own registry read; existing hosts require rollout/rotation |
| #740 | Three runtime Compute admin roles exceed provisioning calls | Exact calls support narrower custom roles; network/subnet/router name conditions unsupported; network isolation remains unresolved |
| #741 | Runtime storage.admin retains bucket IAM/configuration authority | No current bucket policy writer; conservative objectAdmin/bucketViewer replacement |
| #742 | API reads inventory by default, ESO reads whole project, PAT unused | Runtime consumes ESO K8s secrets; API-only ESO uses 29 refs; worker variants need separate coverage; PAT deletion must remain guarded |
| #743 | QA publisher can replace versioned objects and write application images | create-only helper already exists; latest.json AND ISO cache are mutable; separate builder registry needed |
| #744 | QA customer bucket lacks explicit public-access prevention | QA state/foundation/delivery buckets already enforce it; no inherited organization-policy claim |
| #745 | QA runner project secret admin/accessor and actAs are broad | Four DB versions require payload refresh; exact attachments exceed issue's four identities; per-root/deny isolation not implemented or implied |

Read-only direct project policies were refreshed in QA, staging, prod and admin.
QA still has the broad image Compute/actAs/Kubernetes grants, API Compute roles,
ESO project accessor and runner project secret admin/accessor/actAs. This is not
an effective inherited-IAM audit; no token impersonation, secret payload, cloud
write or live positive/negative permission experiment was performed. Raw policies
stay in private local audit storage, outside Git. Historical 2026-10-22 claims
below are not current provisioning status.

Implemented repairs cover each row above. The existing accepted central release
and trusted-main administrator boundaries remain; #740 network isolation and #745
architectural separation must not be marked closed merely because narrower direct
grants are implemented. Replacement runner reads/attachments and host consumers
must precede withdrawal of broad grants; use the ordered rollout in the
[audit report](../../../docs/production-pipeline-iam-review.md#ordered-rollout-after-review).

Independent review found a P1 upload regression: object-prefix conditions do not
authorize the destination listing performed by gcloud/gsutil, affecting fresh
worker versions, QA ISO/latest writes and admin app-release publication. The
design was amended before repair to permit bucket-scoped names/list-metadata
visibility through a custom role containing only `storage.objects.list`.
Payload and mutation conditions remain unchanged. Incremental independent review
at `45895991f` confirms this finding resolved in source with no further actionable
defect. The reviewer inadvertently read historical evidence embedded below the
governing design section; it did not read this review or saved audit/test artifacts
before recording its initial findings. Treat it as an independent source review,
not a perfectly evidence-blind experiment.

Claude Code's first consolidated review identified four applicable corrections:
retain the default-account cache reader while adding the new host reader; create
replacement API object/metadata grants before withdrawing storage.admin; make
the pre-merge QA trigger pause explicit; and preserve image-family lookup with
`compute.images.getFromFamily`. All four are implemented. The first two rollout
corrections were also independently checked against the source.

Other candidates were adjudicated against actual consumers and primary evidence:

- Normal `instances.delete` with auto-delete disks requires instance deletion;
  the separate disk-delete permission is for forced deletion of preserved disks,
  which the runtime does not request. No unnecessary disk-delete grant was added.
- Repository-wide module inspection finds only QA and validated staging/prod
  consumers. No unpublished external consumer compatibility is claimed.
- Both live QA Terraform triggers' target reader inputs satisfy the new allowlist;
  only selected reader fields were retained as private evidence, with no mutation.
- The GKE module has no data sources or child modules to defer through its new
  dependency. Retain fresh-install attachment ordering.
- The registry-token issuer's only production factory is API AppState and its
  mint consumer is HTTP; hosted API configuration supplies the required setting.
  No independent non-API token consumer was found.
- No pipeline consumes the bootstrap-builder image being moved; actual CI
  builder/cache image references retain their existing repository paths.

Combined local verification at `fccb83dcf`:

- All 14 affected Terraform roots/modules validate; 121 provider-mocked tests
  pass, with no failures/skips. The final six affected roots were rerun and
  dependency hashes matched for the other eight roots.
  These are local mock plans/apply, never live Terraform operations.
- 39 focused Rust cloud-provider tests pass, covering identity validation,
  explicit boot-disk naming and both initial/fallback provisioning requests.
- 274 Python tests pass across delivery, planner, runner, release, publication,
  ESO and worker-guard suites. This includes the real local ORAS roundtrip and
  nine host-identity Helm renders; the shell freshness/worker-image matrices
  also pass. The final publisher subset was rerun after its changes.
- The combined run exposed an overbroad pre-existing planner test: it rejected
  an attachment dependency reference as though it were an IAM grant. The test
  now rejects actual IAM resource declarations; the full suite passes.
- Rust formatting, affected Terraform formatting and whitespace checks pass.
- All 48 unique permissions in the audited custom roles occur in Google's
  supported-permission metadata query. This is permission-catalog evidence,
  not an effective-IAM or resource-condition execution test.

Actual Claude Code review completed in three passes: consolidated source,
repair/adjudication follow-up, and closure of its final conditional daemon
question. Final result: no actionable findings. The daemon's constructor results
are inert; all host lifecycle loops start in `run_http_server` for api/all, while
MaintenanceRunner has no cloud dependency. The chart's base daemon.env sets
`G5_ROLE=daemon` and all hosted overlays preserve it. No speculative daemon
permission/configuration changes were made. The GKE dependency concern also
passed a two-case mock experiment preserving known endpoint/no-op resources;
its certificate output was omitted because the mock did not populate master_auth.
No live GKE behavior is inferred from that experiment.

Current-head
hosted CI and live reduced-permission publication, secret sync, Terraform refresh,
host Start/cache/telemetry/deletion and denied-operation checks remain outstanding.
Organization/folder inheritance and transitive privilege paths remain outside
the verified evidence. No cloud apply or merge was performed.

### CI and CodeRabbit follow-up

The staging/prod checks at `4b2ed5ab4` failed during standalone ESO module tests:
unconstrained initialization selected Helm 3.3.0, incompatible with the module's
v2 `set` blocks. The existing callers constrain Helm to `~> 2.12`; the module now
declares the same constraint. Fresh registry resolution with CI's Terraform
1.15.3 selects Helm 2.17.0, and both ESO and release-addons validate with all 22
mock tests passing. Earlier local provider-mirror tests did not expose this
dependency-selection failure.

All eight CodeRabbit comments were adjudicated. Fixes clarify existing prose,
derive target API image readers from the shared project map, assert every planner
secret grant's identity/role/project, and explicitly document guarded retirement
of the obsolete PAT secret container and versions. Reapplying delivery enables
its managed triggers; it does not remember which were previously paused.

The confirmed QA freshness gap is repaired with the existing predicate shared
across root/apply boundaries and immediately before downstream release submission.
Failed/cancelled successors still supersede the older build; successful skipping
is cancellation, not proof the successor applied. No retry or new authority is
introduced. Actual YAML process fixtures cover between-root supersession, final
changed/no-change roots, late lookup failure and publication, in addition to the
existing entry/pre-apply cases.

Final follow-up evidence: 32 QA delivery/platform mock tests pass, including the
helper's trigger path and distinct planner/runner identities; 86 combined Python
tests pass (15 producer process tests and 71 release/order/delivery tests).
Touched Python contract tests have concise docstrings with AST-equivalence
checks confirming no behavioral changes.

Actual Claude Code reviewed the sanitized incremental diff/source, then the
corrections. Its documentation finding is fixed: the plan deletes the secret
resource, whose deletion also destroys externally seeded versions; versions are
not separate plan entries. Its optional test concern is fixed with distinct
service-account defaults and exact overrides. Final follow-up result: no
actionable findings. Hosted CI for the new commit remains pending. No cloud
apply, live IAM/DNS change or merge was performed.

### Preview E2E failure and final IAM test coverage — 2026-10-27

At `1e4bb0770`, QA/staging/prod Terraform, Rust and CodeRabbit checks passed.
[E2E build 6a35d8de](https://console.cloud.google.com/cloud-build/builds;region=us-central1/6a35d8de-480b-449b-b0e6-c36253ef6e3f?project=g5-qa-1ccc)
failed receipt collection after the preview run-log test missed
`G5_NOISY_SHUTDOWN` on its first attempt and passed on retry. Both normal browser
shards completed; the strict receipt correctly rejected retry-only evidence.
Neither that policy nor the test's timeout/fixture command is relaxed.

The pre-existing shutdown code waited for the shell, then immediately killed the
remaining process group. A real shell/child/hosted-PTY regression reproduced lost
shutdown output: the shell exited in about 90 ms while the child still handled
SIGTERM. The repair preserves the remainder of the original group deadline
before the final kill. Empty or zombie-only groups must return promptly, while
unreadable process state conservatively retains the grace. Lifecycle stops still
use zero grace. This fixes the reproduced race; the original failed run has no
first-attempt trace proving its exact signal ordering.

The remaining CodeRabbit comment identified two IAM guards covering only member
resources. They now reject forbidden member/binding grants and project policies;
the latter may hide roles behind referenced `policy_data`. All 18 delivery tests
pass, and six injected project-scope mutations are rejected. Documentation was
added to the touched Rust and shell functions without changing their behavior.

Final local evidence: 30 preview tests, 15 shared termination tests and 15
bundle-producer tests pass. The preview tests exercise real processes and a
hosted PTY, shutdown output, cleanup ordering, immediate lifecycle stop, prompt
empty-group completion and the original deadline after a slow shell. Synthetic
Linux process-stat cases check zombies, live threads and uncertain reads. The
new isolated Linux subreaper regression awaits CI on this macOS host. Local Rust
execution required a temporary `ptsname_r`/`ptsname` portability shim in a copied
test tree; that unrelated shim is excluded from the patch. These results do not
constitute Linux E2E acceptance or a new descendant-reap guarantee.

Independent review and Claude Code found the initial zombie-only latency gap
and indirect-policy test gap; both are corrected. Independent review also made
the Linux orphan fixture deterministic, and Claude required a completion marker
so an empty test-name match cannot silently pass. Final independent source review
found no blocking defect; Claude reviewed the final test delta and reported no
actionable findings. Formatting and whitespace checks pass. Hosted Linux CI on
the published follow-up remains required. No cloud apply, live IAM/DNS mutation
or merge is part of this follow-up.

## Main rebase verification (2026-10-27)

Rebased all 23 PR commits onto `ba8f76c7c`; 20 retain equivalent patches.
The Terraform conflict preserves the IAM assertions and main's assertion that
the email allowlist is retired. The preview conflict preserves the process-group
guard, original shutdown deadline, zero-grace stops and zombie-aware handling,
with both branches' tests retained. Main's release-canary scripts are unchanged.

Validation passed: 183 focused Python tests, 18 mocked Terraform tests,
32 preview tests and 15 shared process-termination tests, plus Rust formatting
and whitespace checks. The Rust tests used an isolated copy with the temporary
macOS portability adapter described above; no adapter is committed, and hosted
Linux CI remains required. Claude Code reviewed the conflict resolutions and
reported no blocking findings. Its low-severity observation about an inherited
Linux test's zombie detection remains a CI diagnostic consideration. No cloud
apply, live IAM/DNS change or merge was performed.

## Delivery readiness follow-up (2026-10-22)

The owner requested execution of the delivery checklist while the existing
no-merge instruction remains in force. Read-only checks, local fixes and PR
updates are in scope; cloud applies, DNS publication and release activation
remain pending. Backups may be stored privately on the owner's local computer.

Initial review findings:

- CodeRabbit questioned the empty legacy object-reader bindings. Inspection and
  a real Google provider 5.45.2 loopback execution confirm the request contains
  the role without members; no live Storage API rejection has been established.
  The design is amended to retain only existing Project Owners, avoiding that
  compatibility risk while excluding Project Viewers and preserving other roles.
- The target-reader mock test checked Compute members but did not assert each
  Artifact Registry and worker-artifact bucket member. Add exact mapping checks;
  the actual member mappings are correct by inspection.
- Fresh Cloud Build PR plans cannot be invoked through RunTrigger for this v2
  repository PR trigger. Application-default credentials are expired; investigate
  read-only local plans using the already-authenticated operator identity.

Completed verification:

- 58 affected provider-mocked Terraform plans passed: QA bootstrap/platform,
  admin bootstrap, target bootstrap/platform, and delivery reader mappings.
- Real Google provider 5.45.2 against a local HTTP model emitted the exact
  owners-only role, removed Project Viewer from it, preserved unrelated
  roles/conditions and the etag, and produced a no-change repeat plan. The empty
  negative control was rejected by the model only; this is not live GCS evidence.
- Authenticated Claude Code reviewed the readiness delta with no actionable
  findings. The two live QA buckets already grant Project Owners legacy object
  ownership; the retained reader membership does not expand their effective access.
- QA trusted HTTPS returned 200, its certificate was ACTIVE, and its certificate
  map was attached. Cloudflare remains authoritative for QA; all four Google
  nameservers match its app/preview A and two validation CNAME records.
- Admin/staging/prod projects and billing are active. Required infrastructure
  APIs remain disabled and the new projects have no buckets. Target quotas,
  runtimes, callbacks and acceptance are not verified.
- Four generation-pinned QA remote states, both available local bootstrap state
  files and the complete six-record Google zone were saved outside Git in
  owner-only local storage. All 20 backup files passed structure/size checks
  where applicable, SHA-256 and permission verification. Local bootstrap
  freshness remains unverified. This is a backup, not a restore rehearsal;
  full Cloudflare inventory remains unavailable to the owner.

Fresh operator-authenticated QA plans at #706 `481e2d9b` and #707 `2b01cbc9`:
foundation and addons report no changes; delivery proposes four in-place trigger
updates. Platform refresh fails with a 403 reading IAM on the existing
`g5-secret-rotation` Pub/Sub topic. Its partial plans show the three intended API
IAM deletions but are not successful plans. Existing planner impersonation is
also unavailable to the operator; no IAM grants were changed. These checks use
`-lock=false` and never apply or write remote state. #703's current Cloud Build
plan at `9af8a24` passed with DNS output additions only.

The #735 readiness working tree (tracked-infrastructure digest
`0ab43972f6a1fc1db13995be5187e307590ee9c887af746c559cd8c97f639432`)
was also planned against QA. Foundation successfully plans the image-publisher
role replacement and removal of two broad image-builder grants; delivery has
the same four trigger updates and the `addons` root has no changes. Platform remains blocked
by the same Pub/Sub read, so its partial bucket/API IAM changes are not a complete
plan. Do not use partial plan files for an apply.


| Delivery item | Status / remaining prerequisite |
| --- | --- |
| Code review and focused fixes | Reviewed and locally verified; current-head hosted CI remains required after publication |
| Two human DNS reviews | No human approvals recorded on the four draft PRs |
| Complete refreshed QA plan | Platform read blocked by operator Pub/Sub IAM permission; run with the existing authorized CI planner or an authorized operator |
| Recovery inventory | QA remote/local bootstrap states and Google-zone backups complete; full Cloudflare export requires its administrator |
| Merge and QA IAM rollout | Not performed; bootstrap must precede foundation/platform, with reviewed IAM removals and normal deletion guards retained |
| New environments and admin artifacts | Not provisioned; no release identities, target state, runtimes or admin artifact transition applied |
| Public DNS/TLS activation | Requires a Cloudflare administrator and two DNS reviewers; QA remains unchanged, new hostnames return NXDOMAIN |
| Canary, restore, notifications and weekly train | Require deployed environments, fixtures and activation; not marked complete |

No merge, live IAM/DNS mutation, deployment or release activation was performed.

## IAM audit follow-up (2026-10-22)

Scope: the separate follow-up on #707, governed by the existing plan's IAM audit
amendment and operations `environments`, `sensitive-storage` and
`artifact-authority` commitments. This change implements the remaining image
publisher, sensitive-bucket, API CI-secret and fallback-reader reductions. The
API-wide impersonation and planner-state reductions remain inherited from #706;
the live audit confirmed they have not yet been applied.

Authenticated Claude Code reviewed the implementation in two read-only rounds.
The first found that a target-project data lookup would break QA plan/apply
because those identities lack target metadata reads, and requested a distinct
cross-environment Compute rejection test. The design was amended before replacing
that lookup with audit-confirmed immutable project numbers and adding the test.
The second review reported no actionable findings and confirmed both fixes.
No additional IAM grants or deployment coordinator were introduced.

Executed local checks: 69 Terraform mock plans across QA bootstrap/foundation/
platform/delivery, target bootstrap/platform, admin bootstrap and secrets; 91
Python regression tests (production delivery, planner permissions, worker artifact
publication, resolution and waiting); five Helm rendering checks. One optional
ORAS integration test was skipped. Changed Terraform formatting and diff checks
pass. Tests use mock providers/command doubles; they do not establish cloud access.

No cloud apply, merge, impersonation attempt or DNS change occurred. Live role
removal (the empty-member approach is superseded above), image publication
under reduced permissions, workspace Start/save/restart, host artifact/cache reads,
and effective inherited IAM remain acceptance work after reviewed application.
Organization IAM was inaccessible during the audit. The operator rollout and
unchanged broad trust decisions are recorded in the IAM review document.

## Staging hostname rename — 2026-10-22

The owner selected `lean.g5labs.ai`, with `*.preview.lean.g5labs.ai` previews.
The existing design and `delivery.dns`/`delivery.dns-review` commitments were
amended before dependent code. #703 changes design/runbook references; #706 also
updates setup examples and the secret callback-origin guard; #707 updates child
composition and all runtime hostname consumers. Project/resource IDs, QA/prod
names, permissions and release behavior remain unchanged.

Review scope: new child/preview/callback agreement, old-child rejection, private
configuration and namespace boundaries, unchanged QA/prod IDs and the existing
zone replacement guard. No live DNS, certificate or callback changes are claimed.

Authenticated Claude Code reviewed the narrow rename in all three PRs with no
blocking findings. Across the stack, 57 Python tests, five real Helm rendering
checks and 49 provider-mocked Terraform plans pass. The bootstrap test initially
had a module-path setup error; rerunning that module from the repository root
passed all six cases. Old hostnames remain only in provenance and rejection
fixtures. No live namespace, provider callback or migration verification is claimed.

## Option B implementation — 2026-10-22

The owner requested implementation after selecting the delegated DNS design.
This supersedes the historical QA cleanup/manual parent-record conclusions below.
The prior design-only handoff remains in Git at #703 `2e0291f55`, #706 `bdf7cf570`
and #707 `a9a836dec`. No merge, cloud apply, live IAM grant or DNS change occurred.

### Review inventory and evidence

| Commitment | Actual path and discriminating check | Result / limit |
| --- | --- | --- |
| Parent/child authority; preserve QA identities | Shared networking resolves an explicit zone, rejects out-of-child names and preserves original record keys; QA foundation keeps its existing zone address | Local provider/mock checks pass; current public QA authority remains Cloudflare until the separate NS switch |
| DNS lifecycle independent of ingress | #707 target platform always owns the protected child; configured hostname inputs remain when ingress is disabled | Target mock plans compare private/public record and map identities; empty private config has zone only; explicit hostname removal is still DNS/TLS teardown |
| First creation and certificate records | Shared record keys depend on configured domains, not computed authorization values or new-zone outputs | Fresh target-zone plans and mock authorization cases exercise unknown values; real certificate issuance remains unexecuted |
| Environment namespace and compatible configuration | #707 platform, addons, delivery input and application release validator restrict names to their environment child; allow empty private setup and in-child renames | Consumer/plan checks cover both allowed forms and cross-child/lookalike/equal-name rejection; inspect all four consumers for consistency |
| Existing identities and least additional authority | #707 target bootstrap enables DNS and grants existing release identity DNS admin; planner grants unchanged | Bootstrap mocks pass; read-only Google role metadata confirms existing planner zone/record reads; effective deployed IAM and denied writes remain unexecuted |
| Release and QA behavior | #706 inherits QA preservation; train, approval, per-target PR plans and direct-port release sequence are unchanged by this DNS amendment | Focused Python and real Helm regression checks; hosted CI and real release/app acceptance are separate |
| Review, TLS activation and recovery | Runbooks require two human DNS reviews, restricted backups, child NS/DS/CAA/TTL checks, manual map attachment and retained serving resources during reversal | Code and runbook review only; no human approvals, NS publication, target HTTPS or customer cutover claimed |

### Initial findings and repairs

- Reused networking deletes its authorization/certificate inputs when hostname lists
  are cleared. The design was amended before dependent code: configured names now
  prepare and retain DNS/TLS independently of target ingress, without a new flag.
- Self-review found target platform's inherited hostname regex accepts malformed
  in-child labels while other consumers reject them. Align its syntax validation
  with the existing delivery/app rules and add a private-configuration rejection
  case. The corrected platform validation passes all 19 target mock plans.
- Actual Claude #703 implementation review found no blocking defect. Its bounded
  follow-ups cover non-CNAME authorization rejection and disabled-IP absence of
  zone lookup/record sets. The authorization dependency remains intentional so
  scope validation precedes authorization creation. All 11 networking mocks pass;
  actual Claude follow-up reports the bounded delta clean.

### Local verification results

- #703: 11 shared networking provider-mocked cases, one QA bootstrap case and
  one QA foundation case pass; QA bootstrap/foundation/platform validate.
  Original QA zone and four record addresses are retained by source inspection.
  Actual Claude implementation review and narrow follow-up have no remaining
  actionable findings.
- #707: 19 target platform mock plans, eight bootstrap mock plans, 17 addon mock
  plans and 13 delivery-root mock plans pass. Both actual target platform roots
  validate with backend disabled. These are model checks, not cloud operations.
- Consumer regression run: 156 Python release/train/planner tests and five real
  Helm rendering checks and 27 pipeline dependency checks pass. The in-child rename fixture matches the new scope.
- Refreshed QA plan was attempted read-only with locking disabled. Terraform ADC
  rejected the GCS backend read with `invalid_grant / invalid_rapt`, before planning;
  refresh-plan preservation is therefore unverified, not passed.

### Incremental integration review

Actual authenticated Claude Code reviewed #706 after restacking the restored QA
DNS dependency. It found no actionable issues: DNS API/runner grants coexist
with the planner lock-only policy, Scheduler grant and API impersonation hardening;
the private target caller remains compatible; release/train/PR-plan behavior is
unchanged. Its 10 private-platform mock plans pass. Two subsequent wording fixes
clarify the artifact transition is not activated and the dedicated planner has
explicitly authorized state reads; they do not change behavior.

Claude's initial #707 review questioned whether a fresh zone's data lookup would
run before zone creation. Exact Terraform plan JSON shows `actions=[read]` and
`action_reason=read_because_dependency_pending` for that lookup while the zone is
planned for creation, including private preparation. The concern is disproved;
no broader module dependency is added. The malformed private hostname discrepancy
was fixed and its regression passes. The temporary missing QA inputs were resolved
by inheriting #703 and preserving #706's hardening during restack. Actual Claude
follow-up withdrew the deferral finding, accepted the syntax fix and inspected
the restored QA inputs; no blocking implementation defects remain. A final
source comparison confirms QA roots exactly match #706 and shared networking
matches #703. QA bootstrap/foundation/platform and both target platform roots
validate offline; live refreshed-state verification remains blocked as stated.

### Outstanding operational verification

| Governing ontology | Gap | Proposed action | Limit |
| --- | --- | --- | --- |
| `delivery.external-dns` | Refreshed live-state plans and final QA preservation | Reauthenticate Terraform ADC, then run and review fresh QA plans before apply | GCS backend read failed with `invalid_grant / invalid_rapt` before any plan; no state lock/write was attempted |
| `delivery.dns`, `delivery.dns-review` | Human DNS reviews and live child delegation | Review, prepare records, then switch NS using runbook | This task authorizes code only |
| `delivery.external-dns`, `delivery.production` | Effective target IAM, TLS and app acceptance | Verify reads/denied writes, ACTIVE cert/map, login/workspace/agent/preview before promotion | New targets have not been deployed |

## Option B design review — 2026-10-22

Authenticated Claude Code reviewed the shared DNS amendment and each PR's
specific plan/runbook changes, then rechecked the narrow corrections. No
actionable findings remain in the documentation. Corrections remove stale
parent A/CNAME publication, specify DS and cache-aware recovery, preserve exact
QA state addresses, and clarify DNS review and private-zone/public-validation
ordering. The historical IAM baseline is retained. Provenance follows the
repository taxonomy and does not imply human review approval or live verification.

The final amendment changes Markdown only. Relative file links, English additions,
ontology block/metadata pairing and diff checks pass. Runtime files are unchanged
from each PR's previously reviewed head; Option B implementation, refreshed plans
and live IAM/DNS/TLS/application acceptance remain pending. No merge or cloud apply.

## Historical review evidence

## Cloud DNS cleanup dependency

#703 now removes the unused QA Cloud DNS mirrors, zone and DNS-specific bootstrap
configuration. The release-platform caller drops its obsolete mode argument.
QA DNS retirement must finish before the pipeline bootstrap removes its runner’s
DNS role. The final stack passes 13 release-platform mock plans, 13 delivery/IAM
regressions and five real Helm tests. The updated network/bootstrap cases and QA
root validation are recorded in #703. No cloud cleanup or merge was executed.

## Rebase onto main a6e8aa7ee

Preserved main’s QA deploy-ordering commitments and off-path companion work.
The extracted queue helper retains the release line’s pending-approval exclusion;
QA releases its turn after smoke tests, while staging/prod retain theirs through
acceptance and promotion. Promotion joins publication confirmation and cache
warming so its child can verify a completed producer. DNS implementation and both
hostname commits are unchanged. Independent reconciliation review found no defects.

Validation: 156 queue, pipeline, release/provenance, IAM-boundary and train tests
passed on the final stack, plus all five real Helm rendering tests. Ontology blocks
and metadata remain paired. Full CI results from before the rebase apply to the
previous heads; the rewritten commits require new CI. No merge or cloud apply.

This is the incremental self-review of the existing production-pipeline design;
local tests are not cloud deployment or restore evidence. The direct-port amendment
below supersedes the earlier custom Terraform/SHA-gate review findings and evidence
where those mechanisms have been removed; unaffected auth/frontend evidence remains.

## Initial findings

- **Release configuration prerequisite:** application preflight accepted a
  successful Terraform build with matching SHA/target/artifact owner without
  proving it used the current target configuration. The Terraform consumer also
  validated project/region but did not bind its settings or approval state to
  current trigger policy. A changed or overridden configuration could therefore
  satisfy the infrastructure-before-application gate incorrectly. Repair the
  existing shared validator and check the applied settings at the application
  consumer; retain same-SHA retries under unchanged reviewed policy.

## Evidence and limits

The former strict staging-admission evidence is historical; the owner removed
that feature in the login-policy amendment. Current scoped evidence is recorded below. No cloud apply, merge, public DNS
publication or live customer acceptance is part of this review.

## Initial core consumer findings (2026-10-21)

The independent core release review received the explicit hint that DNS/TLS and
operations were known work in progress and excluded those areas. Missing live
acceptance was not treated as a coded defect. Before repair it found:

- **P1 — Freshness environment handoff:** the actual Cloud Build freshness step
  omitted artifact-project and release-bucket settings. Its staging/prod delegate
  instantiated the release helper with an empty required bucket and failed before
  Helm. Running the rendered shell step reproduced the refusal; prior step exports
  do not persist across Cloud Build containers.
- **P2 — Manual infrastructure prerequisite:** application preflight verified only
  its current target. A manual staging approval could therefore change the app
  while production infrastructure was missing or failed, contrary to the governing
  requirement that both target applies precede application delivery.
- **P2 — Full Terraform history stall:** freshness refused every full 1,000-build
  result before excluding pending candidates. Main-per-SHA staging queues would
  eventually make every release and retry fail even with unambiguous prior apply
  evidence. A hermetic history containing 1,000 pending records reproduced it.

The initial focused release suite passed 112 tests despite these gaps. Repairs
and discriminating consumer/history regressions follow within the same design;
no cloud operations, repository publication or new approval were performed.

## Core consumer repairs and evidence

Repaired the three findings without adding release features, permissions or new
storage. The freshness step now exports its artifact layout. Both staging and
production application preflight require successful staging and production applies
at the candidate SHA, each checked against its own current trigger policy. The
Terraform history query excludes unapproved pending and unstarted expired builds
before its limit, orders the result newest first, and permits a full window when
the newest prior started apply is resolved. Unresolvable prior source/order or a
full window without prior apply evidence still refuses release; failed started
applies continue to constrain forward recovery.

The history query was checked against the installed gcloud filter rewriter,
without calling GCP: `trigger_id`, `start_time` and `status=QUEUED` rewrite entirely
to the server filter. Queued records are then checked for actual approval in the
script. This avoids mixing a server-only field name with an unsupported client
approval-state filter.

All 139 focused release, train, provenance, delivery and Cloud Build consumer tests
pass. New coverage executes the actual rendered freshness shell and real Python
consumer for both targets and both artifact owners, exercises missing/failed/stale
infrastructure for either target under manual staging and production delivery, and
checks full history, latest failed apply, newer approval and ambiguous evidence.
`git diff --check` passes. These are hermetic tests with cloud boundaries stubbed;
no IAM, deployment, live acceptance, publication, merge or commit occurred.

## Scoped completion

The initial configuration-prerequisite finding is repaired by the shared Terraform
trigger validator and both-target application prerequisite. Settings, approval,
identity and chain mismatches fail before use; matching same-SHA retries remain
supported. The release suite above covers these paths. Frontend root routing
passed 88 component/auth and five browser checks, and secret helpers passed 21
subprocess tests. The later login-policy amendment removes the strict-admission feature.

The owner removed the proposed extra backup/monitoring automation from scope.
Its 15 new operations files and two backend log additions were removed; existing
Cloud SQL backups/PITR and Cloud Build notifications remain. The rejected new
Scheduler permission request is withdrawn rather than approved.

| Governing contract | Verified path | Remaining evidence |
| --- | --- | --- |
| Release flow and exact settings | Both target prerequisites; immutable producer/consumer/approval tests | Live Cloud Build and IAM |
| QA compatibility | Rendered freshness step, legacy-trigger QA defaults and rejection of artifact overrides | Explicit trusted policy before admin ownership or release-bucket use |
| Staging admission | Auth implementation/tests match the PR base; scoped configuration validation below | Real provider registration and deployed login |
| App entry and provider configuration | Browser routing and actual shell helper subprocess tests | Deployed callback, workspace and preview flow |
| DNS ownership | Separate module PR; no duplicate networking change in pipeline PR | Two human DNS reviews, manual records/TLS attachment and serving acceptance |
| G5 backup baseline | Existing Cloud SQL module retains configured backups/PITR | Target restore exercise; old-prod live status inaccessible |

## Review branch verification

The review branch is based on DNS PR #703; it carries no duplicate
networking-module diff. Its implementation matches the tested source worktree,
apart from the documented comparison base and that inherited DNS dependency.
The packaged release/artifact suite passed 158 checks; the optional real local
OCI roundtrip passed separately with ORAS enabled, completing all 159 cases.
Both Helm rendering checks passed. A separate read-only packaging review found
no removed operations code, credential patterns or generated artifacts among
the 118 changed/new files. No cloud operation or merge was performed.

## Direct G5 port and weekly preference

The latest user instruction supersedes the additional Terraform/SHA policies above.
The train script matches G5 `8458df8190f5` byte-for-byte after the two dev-to-QA
trigger-name substitutions. Terraform uses the upstream OCI carrier, digest
materialization, tests, plan/apply and chaining, with Lean project/path/root
bindings. Native platform/addons state and ordering remain; the GCS source
protocol, cross-root lock and additional downstream refusal policies are removed.
The app consumer has no separate infrastructure-history gate. Admin Terraform
publication now uses the existing `g5` registry, replacing GCS source-bundle writes.

The weekly preference is true in variables, examples and QA apply configuration;
train permissions and main-pinned train/Terraform pipeline configuration also
match G5. The nine topology contracts pass after that final parity adjustment.
the effective train requires both targets and enabled promotion. Live inspection
found only QA triggers and a disabled Scheduler API, so no live schedule was
activated. Existing explicit false trigger substitutions must become true during
the documented deployment. No merge, cloud apply or DNS change was performed.

All 132 CI-wired train/producer/consumer/app/topology tests pass with Python site
packages disabled, including actual shell steps against local external-command
doubles. The broader release/artifact/Helm suite passed 151 tests before adding
the six separately passing producer cases. Nine mocked admin IAM plans pass,
and QA delivery/admin schemas validate. Independent read-only review found no
defect in digest handoff, IAM, root ordering, failure propagation or retained QA
guards. Full cloud release/workspace acceptance remains unexecuted.

The previous PR CI failures were test regressions: an overly broad JWT fixture
type and a reset-guard fixture missing the QA environment. Both are fixed; the
TypeScript build, 44 App tests and 17 reset tests pass. The reset script's actual
QA-only behavior is unchanged. CI now runs the release shell and train regressions
in the existing Rust pipeline with G5's tzdata setup.

## Claude Code review follow-up

Claude Code identified a legacy QA trigger rollout deadlock and a missing fixture
flag in the promotion examples. The two producer policies now interpret absent
trigger artifact-owner/release-bucket keys only as QA ownership and an empty
bucket. Resolved build and process values must still match; caller overrides
cannot select admin artifacts. Both target examples enable their referenced
release-check fixture. The setup runbook explicitly retains the QA destroy guard
and requires review of the three legacy IAM deletions before its one-time override.
All 135 CI-wired release tests pass, including the new regressions.
Claude Code's second review confirms both fixes and finds no additional
actionable issues beyond the pending permission below. The first live build must
still verify that Cloud Build's resolved build record contains YAML-default
substitutions; local tests do not establish that hosted-service behavior.

The owner subsequently approved the specific `roles/cloudscheduler.admin` grant
for `g5-lean-terraform@g5-qa-1ccc.iam.gserviceaccount.com` as code only. The QA
bootstrap now includes that missing G5 runner permission, resolving the weekly
job provisioning finding. The weekly contract checks that the trusted runner
receives it and the PR planner does not. Live grants and scheduling remain
unapplied. This is separate from the withdrawn backup-scheduler proposal above.

Claude Code's third pass confirms the Scheduler finding is resolved with no
remaining actionable code findings. The 13 focused delivery/planner checks,
Terraform validation and the existing mocked QA bootstrap plan pass.

The completed remote runs found one stale organization-create E2E expectation in
both #706 and #707: closing the wizard correctly navigates to the new org's Base
workspace, while the test still waited for the retired marketing landing route.
The test now waits for the created org's actual route and keeps its duplicate-slug
assertions. All 44 App unit tests pass and the targeted browser case is discovered.
The full browser rerun requires CI's backend/auth fixtures, unavailable locally.
Claude Code also reviewed this test correction and found no actionable issue:
the new org is active before Step 2, and the URL wait is installed before Close.

## Shared staging login policy

The owner explicitly removed the employee-only staging login restriction. The
four backend auth/config/test files and user-admission ontology now match the
PR #703 base exactly. Staging uses an empty allowlist by default and retains the
shared configurable policy; addons still reject admission settings that differ
from applied platform state. Deployment approvals, QA's existing allowlist,
production defaults and DNS behavior are unchanged.

Validation passes: 23 provider-mocked Terraform plans (10 platform, 10 addons,
three staging bootstrap), schema validation for those roots/modules, two real
Helm rendering tests, and 134 release/delivery/deploy-order regressions. Tests
cover omitted, empty and custom allowlists, stale platform/addons disagreement,
and absence of the strict flag while retaining disabled E2E token issuance.
Independent scoped review found no remaining issues. Backend callback tests are
restored to baseline; they were not rerun locally for this removal. Deployed
provider/login acceptance and full CI on the updated commit remain outstanding.
No cloud apply, DNS change or merge occurred.

## Latest Claude Code review: initial release identity

Claude Code reviewed the complete PR at `e342a4a92` and found a first-activation
planning defect: artifact IAM instance keys used the new release account's
computed email before that account existed. The fix derives known member keys
from the configured account ID and project, with resource-valued members retaining
the account creation dependency. Existing IAM addresses, members, roles and
project scope are preserved; no new permission or cloud operation is introduced.

The real delivery root now has three provider-mocked plan regressions: disabled
authority, initial account creation with empty targets, and deduplicated shared
target readers. The account email remains unknown until apply. The original
configuration fails with both reported `Invalid for_each argument` errors;
the fixed root passes all three plans and preserves the existing binding keys.
Ten delivery contract checks also pass. These are local plan-only tests, not
live IAM or deployment acceptance.

Claude also noted that import attempts another create-only upload. Its claim
that this necessarily retransmits the entire tarball was not verified, and it
identified no correctness failure. The existing generation precondition and
content verification are retained; no speculative upload optimization is added.

## Staging and production PR plan checks

Initial finding: the original G5 release line has separate staging/prod PR plan
triggers; this port had apply-time plans only. The requested repair adds the two
checks using the existing source/plan pipeline and explicitly authorized target
state/metadata access for a dedicated planner. QA's current planner and the
release account are unchanged. The owner explicitly accepted state reads that
include generated database passwords, as code only.

Scoped conformance review:

| Commitment | Actual path and discriminating check | Result / limit |
| --- | --- | --- |
| Per-target PR plans | New delivery triggers plus real-root mock plans check source/plan mode, target bucket/settings, paths and independent activation | Six delivery mock plans pass; hosted trigger execution remains unverified |
| No apply/publication/promotion | Actual release YAML exercised for staging and prod with successful and failing platform/addons plans, including populated downstream inputs | All 22 pipeline regressions pass with external-command stubs |
| Separate planning authority | Dedicated QA account and fixed target bootstrap grants; provider-mocked role/member/state/lock assertions and source identity checks | Eight bootstrap mock plans and eleven delivery contract tests pass; live effective IAM and denied writes remain setup work |
| Existing QA/release/DNS behavior | QA planner wiring and release approval/disabled state asserted; no DNS code or existing planner IAM edited | Local checks pass; no cloud change or deployment performed |
| No extra Helm-secret access | Real pinned Helm 2.17.0 provider plans a changed local release with refresh disabled and unreachable synthetic Kubernetes endpoint; refreshing negative control fails | Supports current configuration only; no live cluster acceptance |

Target plans retain G5's non-refreshing state-based behavior and do not audit live
drift. The target operator must initialize both state roots and verify reads,
locks and denied state writes before populating delivery targets. Terraform 1.9
leaves generated account fields unknown in plan-only tests; identity reference
wiring is covered by the source contract test rather than fixture-only claims.
Actual Claude Code incremental review completed with no actionable correctness,
security or integration defects. It checked both target roots, source selection,
identity separation, permissions and setup. The broader 92 Python regressions,
Terraform schema/format checks and ontology metadata pairing pass. The staging
setup guide now explicitly keeps delivery targets empty until both platform and
addons state exist. Live trigger/permission acceptance remains unexecuted.

Final compatibility review found one missed CI consumer: the notifier contract
hardcoded the complete trigger set and PR-trigger subset without
`terraform_release_plan`. The actual test reproduced the failure on the reviewed
candidate. Both expected sets now include the new trigger, retaining the
assertion that every non-PR trigger is watched and every PR trigger is excluded.
All eight notifier tests and three existing QA-planner tests pass. The exact
six-module release CI suite also passes all 145 tests. This is a test-only repair,
not a notification behavior change.

## Staging canary and release-chain verification

The release chain already places `required-release-checks` after deployment and
smoke verification, and `promote-release` has a hard success dependency on that
step. It runs the agent canary followed by the preview canary against the
target-local URL and secret. Production validates a successful staging parent
and reuses its immutable manifest. The weekly train still completes staging and
production infrastructure before approving the staging application candidate.

Inspection found compatibility defects in the reused standalone canary: workspace
cleanup used a removed DELETE route, preview discovery used a removed branch route
and branch ID, API-key instructions omitted the preview proxy's `tasks:read`
scope, and missing positive PATH/HOME evidence could pass. Repairs use the current
workspace lifecycle and workspace/repository profile APIs. Cleanup retries join
one operation, remain bounded, and use force only for disposable canary-created
clones. Existing best-effort teardown policy is retained.

Verification executes the actual canary Bash with real jq and strict HTTP command
doubles, covering current response shapes, bounded cleanup, missing assertions,
preview success and denied preview access. The actual release-step Bash/Python
entrypoints also run with external-command doubles: agent failure, preview failure
and inaccessible fixtures stop before production invocation; success forwards the
same manifest and SHA exactly once. Hard Cloud Build dependencies and the absence
of allowed failure exits are checked separately. All 151 release regression tests,
Bash syntax and the existing supported-surface canary contract pass locally.
Five provider-mocked delivery plans also pass, including full-release target
binding/approval/chaining and bootstrap refusal to promote unchecked staging.

The shared canary changes affect standalone QA operator runs, but do not add a
canary to QA deployment or alter its trigger/DNS configuration. The setup guide now
specifies per-target synthetic fixtures and the secret payload. Real target quota,
model credentials, deployed APIs and preview routing remain unverified until the
first live acceptance run. No merge, cloud apply or deployment occurred.

Actual authenticated Claude Code reviewed the canary changes against current
backend routes, scope checks, lifecycle admission/retry behavior and error shapes,
plus the release dependency graph and downstream DNS/public-origin integration.
It found no actionable code defects in #706 or incompatibility with #707. The
downstream branch must inherit the repaired helper and tests on restack; its DNS
implementation is unchanged. This code review is not live environment acceptance
or human merge/DNS approval.

## Separate DNS configuration review

The DNS follow-up derives Terraform/platform/addon settings and trusted application
trigger settings from the same reviewed environment object. The direct G5 port
amendment removes its comparison against successful Terraform history; there is
no replacement history/configuration checker. Existing trigger/build/process
validation, private defaults, configurable hostnames and Helm API/Git/edge paths
remain. The manual G5 certificate procedure is unchanged.

The initial DNS package passed 64 focused tests and its source worktree passed
26 mocked plans. Those results predate this compatibility update; affected
release and rendering checks are rerun below. Two human DNS reviews, Cloudflare DNS-only
publication, certificate activation/attachment and deployed acceptance remain
pending. No proxy automation or new permission was added.

Compatibility verification after the direct-port rebase passed 61 focused checks:
24 application-release tests, 23 Cloud Build contracts, nine production-delivery
tests and five actual Helm renders. The public-settings case permits configurable
hosts while refusing caller/process overrides, using a cloud stub that rejects
all infrastructure history/configuration queries. Terraform resource definitions
are unchanged, so the prior mocked-plan evidence is retained. No merge, push,
cloud operation or DNS change was performed during this compatibility update.

## Claude Code hostname review follow-up

Claude Code found no confirmed DNS/TLS wiring defect, but could not establish
substitution behavior in Cloud Build's global `options.env`. The five hostname
settings now use the existing documented per-step `env` pattern in the policy,
preflight and Helm steps. The rendering test now reads those actual YAML bindings
before executing the Helm argument assembly; fixture injection no longer hides
missing bindings. All 56 release/Cloud Build/delivery checks and five real Helm
renders pass, including QA and private configurations. Claude's follow-up review
reports no remaining actionable findings. No new release gate, permission or
runtime component was introduced. Live DNS/TLS acceptance and two human DNS
reviews remain outstanding.

After stacking the reviewed QA compatibility/fixture fixes, all 137 release
regressions and five real Helm renders pass. The rebase retains the parent DNS
compatibility contract and current Cloudflare DNS-only instructions. The parent
Scheduler permission finding is now resolved by the explicitly approved code-only
grant and Claude's follow-up review above; no live acceptance is implied.


## Final canary stack validation

After restacking #707 on the canary fixes, all 153 release/train/canary/pipeline
regressions and all 15 delivery-root provider-mocked plans pass. Shell syntax and
the existing supported-surface contract also pass. The helper, release-check
runner, focused shell tests and CI wiring are identical to #706. The rebase retains
both the release-gate tests and the existing DNS namespace/origin tests; no DNS
implementation changed in this increment.

Actual authenticated Claude Code reviewed the new Terraform assertions and final
restack separately, finding no actionable defects in either #706 or #707. This is
source and local-test evidence. The first live staging canary and target setup
remain required; no merge, cloud apply or deployment was performed.

## Release login allowlist review — 2026-10-23

Scope: the owner requested the existing production allowlist and then staging.
Independent source/ontology review and authenticated Claude Code review found no
blocking defect. Shared auth code, module defaults, QA admission and DNS remain
unchanged. The only PR edits are selected configuration examples and the existing
runbooks/design/ontology.

Review findings were addressed by aligning all bootstrap/platform/addons and
release examples, clarifying the membership exception, using explicit per-target
kubeconfig/context, and waiting for the exact Secret field before API restart.
The parser lowercases/trims comma-separated entries and matches domain entries
with the `@` suffix boundary; all verified provider emails can match. The review
identified retained-session/member admission as a scope limit, not a revocation
request. Read-only account audits and live configuration verification are recorded
below when complete.

Both refreshed platform plans change only the allowlist output. The delivery
plan changes only that field in six Terraform triggers, including their full
target-input maps. Staging addons replaces only the manifest-tracking null
resource (no destroy provisioner), reapplying the ExternalSecret in place.
Both running APIs have the selected allowlist, unchanged images and 20000 RPM,
and public health passes. Production briefly returned 502 immediately after the
restart, then recovered. All six delivery triggers were updated; the existing
operator tfvars were synchronized with restricted pre-change backups retained. Fresh allowed/denied OAuth attempts by a separate identity
remain unexecuted; configuration inspection is not that browser proof.

The read-only account audit found one GitHub user in staging and two in
production. Staging had no non-allowlisted OAuth user; production had the owner's
personal Gmail account without organization membership. No accounts, memberships
or sessions were removed. QA public health also passed after the rollout.
Seven edited Terraform examples parse successfully; `git diff --check` passes.

| Governing commitment | Evidence | Limit |
| --- | --- | --- |
| Production and staging admission | Exact platform output and addons manifest deltas; ExternalSecret, synced Secret and each running API agree | Fresh separate-account OAuth denial not exercised |
| Release persistence | Only allowlist fields changed in six triggers; existing operator inputs updated | Deliberately applying an older input file can reopen registration |
| Shared identity/invitation/member behavior | Existing callback/parser inspected, no auth source changed | Existing sessions remain valid; no revocation requested |
| QA/DNS isolation | Refreshed plans change no QA workload, DNS or IAM; QA health 200 | No new QA login session was exercised |

A fresh QA delivery plan after applying the six updates reported zero resource
or output changes, confirming the saved operator inputs match live trigger state.

## QA Terraform supersession review — 2026-10-23

Scope: the existing QA freshness guard and its downstream Terraform release
publisher, against `g5.quality.delivery.qa-terraform-freshness` and the bounded
amendment in the existing plan. Before changing production YAML, the actual-step
shell regression run reproduced the requested defect: supersession at entry and
between roots ended `tf-roots` with failure. New publisher tests also demonstrated
that missing/invalid skip state was not checked. The baseline ran 12 tests in
10.228 seconds, with nine failing subcases and two missing-control-file errors.
Lookup/plan/destructive-plan/apply failure and plan-only cases already passed.
These are local process fixtures, not cloud apply evidence.

The final implementation keeps the existing same-trigger freshness predicate and
both call sites. A confirmed newer build writes `SKIP_TERRAFORM=true` and exits
the root step successfully; the dependent publisher skips before reading the
bundle handoff. Lookup and Terraform failures still fail. Native diff review
covered both producers and the downstream reader, existing step ordering,
plan-only behavior, and the unchanged destructive-plan guard; no remaining
actionable findings.

Verification on the final YAML and test code:

- `python3 -m unittest discover -s scripts -p test_terraform_bundle_producer.py -v`: 12 tests passed (10.517 s), executing the actual inline shell with local process fixtures. Cases cover entry/mid-build supersession, remaining-root/release suppression, current builds, lookup/plan/apply failures, destructive-plan refusal and missing/invalid publisher control.
- From `scripts`, `python3 -m unittest test_terraform_release test_app_release test_approve_release_train test_release_canary test_production_delivery test_cloudbuild_waitfor_order`: 150 existing regressions passed (25.417 s).
- Actual Claude Code 2.1.310 (`claude-opus-5-5`) reviewed sanitized YAML/test source with tools, MCP and project settings disabled: no actionable findings. Its optional extra mid-build flag/log assertions duplicate the already-tested no-publication outcome and were not added.
- `git diff --check` passed.

The tests exercise local bash and executable fixtures; Cloud Build scheduling,
container images and live gcloud responses were not executed. No merge, cloud
apply, trigger update, DNS change or live release occurred. Earlier roots already
applied before supersession remain applied; this change skips subsequent work.

## Container image CVE reporting — 2026-10-27

The approved report-only contract is implemented in the existing artifact-owner
bootstrap/IAM roots and deployment pipeline. The central release account gets
occurrence reads, without scan administration or findings writes. Independent
Terraform/docs review found no actionable issues. Native review checked actual
Google SDK discovery/severity output, immutable reference handoff, failure handling
and explicit/implicit Cloud Build dependency edges. The reporter has no path into
Helm/canary/promotion prerequisites and honors existing supersession.

Validation: 30 provider-mocked Terraform plans, 14 report/parser and actual YAML
shell tests, 31 build-order tests, and 132 existing release regression tests pass.
Fixtures cover severe findings, incomplete/stale results, malformed output,
permission failure, timeout and unexpected reporter exit. The new suite runs in
hosted Rust CI. Terraform formatting and Git whitespace checks pass.

A read-only browser check verified the digest vulnerability-tab route and caught
a generated console-link error; the link and its regression fixture were corrected.
Current QA reports show the API disabled. No API activation, IAM apply, image push
or real production release has been performed. Live qualification still requires
the runbook's repository enablement, release-account reads and new-candidate checks.
The local provider download initially hit disk exhaustion; reuse of the official
checksum-verified provider resolved setup without changing dependencies.

Authenticated Claude Code reviewed the explicitly approved sanitized diff and
source, then rechecked its bounded fixes: timestamp parsing no longer discards
nanosecond-based reports; query/step budgets are 20/180 seconds; console URLs use
the observed digest route; occurrence-read scope and implementation evidence are
accurate. The final review reports no remaining actionable findings. No credentials,
customer data, Terraform state or raw cloud policies were sent.

### Compatibility with pending #735

At #735 `5b9477cc4` and #890 `dc3bc8186` plus this follow-up, the IAM
resources coexist: artifact reads and the central release identity remain;
occurrence-reader grants are additive. The activation runbook now retains #735's
staged IAM transition, trigger pause/freeze and fresh-source requirements. Moving
this CVE evidence section avoids the two PRs' competing top-of-file insertions.
Independent review found no actionable issue in the narrowed rollout correction.

A temporary three-way source fixture combines both PRs and this follow-up without
text conflicts or branch/ref merges. All 50 affected provider-mocked Terraform
plans pass (QA bootstrap 6, QA delivery 29, admin bootstrap 3, admin artifacts 12).
All 175 Python checks pass across CVE reporting, build order, runner attachment
contracts and Terraform/application release, approval, canary and delivery suites.
The first fixture omitted unchanged backend/frontend inputs; restoring those
tracked files resolved its two setup failures. No product fix was needed.
Terraform formatting and Git whitespace checks pass. These checks do not establish
live IAM propagation, completed #735 rollout or a scanned production release.

Actual Claude Code reviewed the sanitized rollout diff, relevant combined IAM
source and synthetic tests. Its target-environment sequencing finding was fixed:
report validation must run within #735's staged target rollout or after completion,
not through a standalone application/infra promotion bypass. A bounded Claude
follow-up reports no actionable findings. Local test and merge-compatibility
results above were produced separately; Claude did not execute those checks.

### Missing scan metadata and partial live IAM rollout follow-up

Missing vulnerability summaries or missing `vulnerabilities` keys now use the
existing incomplete-report path without fabricated zero counts; explicit empty
maps retain the zero-returned-findings caveat. Parser and actual YAML-shell
regressions failed before the fix and pass afterward. All 178 tests in the eight
affected reporter/order/release suites pass. Activation instructions now cover
pending or partial #735 transitions in either selected source or live state.
Actual Claude Code review of the sanitized follow-up reports no actionable
findings. Whitespace checks pass. Terraform and deployment gates are unchanged;
live scan output and release-account access still require activation validation.

## Release Slack notifications — 2026-10-28 UTC

Reviewed against the approval-alert, cutoff-summary, failure-alert, release-flow
and impact-notice commitments. Reused G5's notification code and Lean's train
planner; the unrelated post-deployment notifier was excluded. No release gate,
application behavior or DNS change is included.

Actual Claude Code review found three defects: a stale CI assertion, Terraform
1.9 mock values unavailable during plan, and a late successful skip potentially
moving the production comparison baseline backward. All were corrected. The
baseline now uses bounded QA-origin ordering and makes competing unknown history
explicit. Follow-up Claude review found no blockers; minor fixture/comment and
CI image-pinning suggestions were addressed. The dedicated cutoff identity has
G5's project-wide read-token scope, documented without claiming repository isolation.

Validation: 239 Python tests pass, including the manual-success, approval-only,
old-green-skip and missing-history cases. All 20 delivery Terraform mock tests
pass with a backend-free copy and clean teardown. The persistent Go harness
passes 14 rendering cases. A separate local harness using actual Cloud Build
protobufs, CEL and Slack libraries passes 140 filter combinations, two disabled
approval cases and ten serialized Slack payload cases. The library baseline is
GoogleCloudPlatform/cloud-build-notifiers `80c6cfed3659a31626ffbd68387686bbebd50f67`;
this validates compatibility, not deployed image provenance. CI runs the Python,
Go-renderer and Terraform suites. Terraform validation, formatting and YAML
references pass.

A read-only execution of the cutoff script with the current operator identity
rendered a message from live QA build history, with no failure marker or Slack
POST. It does not establish the new reader identity's effective IAM. New approval
and scheduled-cutoff delivery still require merge, delivery apply and live
acceptance; nothing was merged or applied by this change.

The existing failure path was independently exercised before this change:
[staging](https://console.cloud.google.com/cloud-build/builds;region=us-central1/b9abed6b-3553-47a2-9b4c-7b0ba949ec84?project=g5-qa-1ccc)
and [production](https://console.cloud.google.com/cloud-build/builds;region=us-central1/5c5ec2d7-dbab-448b-b805-0f71d99fb818?project=g5-qa-1ccc)
failed at validation before deployment. The notifier logged a send and successful
request processing for both. Slack UI receipt was not separately inspected.

### CI and webhook-validation follow-up

The initial E2E build `f6f1f4bf-92aa-41f6-9865-0a8eb58bfea1` failed only
the inherited block-edit differential test at its default 15-second timeout;
5,312 frontend tests passed. That exhaustive test now uses the neighboring
corpus tests' 120-second budget, preserving every case, yield and assertion.
All 65 block-edit tests and the frontend TypeScript build pass locally.

Malformed webhook configuration is rejected before request construction with a
safe error naming only the secret/version. Main-entry regressions verify no
secret logging, HTTP post, GitHub token read or build listing. All 241 affected
Python tests pass; Python 3.10 syntax, formatting and whitespace checks pass.
Actual Claude Code review of the explicitly approved sanitized follow-up reports
no actionable findings. No live notification, deployment or cloud apply was run;
the updated PR still requires hosted CI and post-apply notification acceptance.

Rebased onto `9894f69cf` after #890 merged. Both observability contracts and
review records are preserved, and CI runs the union of their suites. All 257
Python tests, 20 delivery mock tests and 14 Go rendering cases pass against
the combined source; Terraform validation and whitespace checks pass. Actual
Claude Code review of the approved sanitized rebase diff found no actionable
issues. Current-head hosted CI and live activation remain separate checks.
