# GitHub PR authorship

## Ontology changes and governance

### Current amendment: PR checks for the VM agent

Implementation design for the [CI loop request](request.md#inspect-pr-checks-from-the-vm).
The user wants the agent to push, inspect CI and read failures before repairing its work;
direct gcloud/Cloud Build access is a separate project. This amendment adds read behavior
without changing the implemented publication/manual-retry contract. The user has authorized implementation.

**Implementation amendment after independent review.** Read both head and the current
open PR test-merge SHA, labeling rows with `commitOid` and `revision` (`head` or
`test_merge`) while retaining `headRefOid` for the source publication guard. Unknown
mergeability remains pending. Revalidate the PR revision vector, suites and latest runs
before accepting a terminal observation. Queued rerequests invalidate earlier completed
results; other active suites retain current failed jobs alongside pending suite evidence. Partial annotation errors retain supplied summaries/text and stop the read;
rate limits stop remaining provider requests and watch polling. These decisions resolve
the saved revision/rerun assumptions; the original discovery and handoff snapshot remain
in [review evidence](review.md#pr-checks--consolidated-findings-before-repair).

**Ontology changes.** Comparison base: `21e0e65eaacf9596e3e80ca98af4585b49743d7f`.
Add `g5.governance.pr.checks`, `g5.governance.pr.checks.revision` and
`g5.governance.pr.checks.details` in [governance](../../ontology/features/governance.g5.md#inspect-provider-checks-from-a-workspace-agent),
and `g5.architecture.pr_checks` plus its public operation row in
[Git/governance architecture](../../ontology/software_architecture/git_governance.g5.md#one-provider-check-read-operation).
Each added block has paired metadata recording implementation and verification limits. No existing
contract is weakened. The exact ontology diff is:

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 354e478e..3c6c795b 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -190,6 +190,60 @@ Governing: [workspace read authority](../concepts/security.g5.md#read-permission
 [current token authority](../concepts/security.g5.md#admit-actions-against-current-authority)
 and [server-side provider credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side).

+### Inspect provider checks from a workspace agent
+
+[D · g5.governance.pr.checks]
+
+`g5d pr checks` reads GitHub check runs and commit status contexts for the selected PR,
+using the existing workspace/provider selector and read-authority rules. A G5 PR spans
+its linked provider PRs; results retain each repository, PR and observed head SHA rather
+than choosing an arbitrary member. Missing publication, access, check data or incomplete
+enumeration remains explicit and cannot become a passing result. Output describes the
+checks observed, not proof that every required check has been scheduled or that merging
+is authorized. Readable output, field-selected JSON and cancellable CLI polling support
+the agent's push, inspect and repair loop without creating durable monitoring state.
+
+Governing: [PR retrieval](#retrieve-pull-requests-through-the-cli),
+[JSON compatibility](#select-pr-json-fields-using-familiar-syntax) and
+[authoritative outcome evidence](../concepts/data_model.g5.md#report-outcomes-from-their-authoritative-owners).
+
+### Bind check observations to the requested revision
+
+[D · g5.governance.pr.checks.revision]
+
+Check reads identify the live provider head and the revision each result describes.
+For an open PR, include checks on its current test-merge commit when available; unknown
+mergeability stays pending. Keep actual check commits distinct from the source head.
+An optional expected head for one selected repository prevents old passing checks from
+satisfying a caller waiting for its push to reach GitHub. A mismatch is pending, not
+success. Watch re-resolves the provider head, discards superseded observations and
+revalidates revision and check-attempt identity before reporting terminal results.
+An unfinished suite keeps its pending evidence separate from current failed jobs;
+a queued rerequest cannot reuse earlier completed results as current.
+A read is an observation at a time;
+it neither reserves the head nor changes version-bound approval or merge validation.
+
+Governing: [successful-push publication](#follow-successful-source-pushes-automatically),
+[reviewed versions](#bind-human-approval-to-the-version-reviewed) and
+[check retrieval](#inspect-provider-checks-from-a-workspace-agent).
+
+### Read the failure evidence published to GitHub
+
+[D · g5.governance.pr.checks.details]
+
+On request, check retrieval includes GitHub-published failure summaries, text and file/line
+annotations, or the description and destination link supplied by a commit status.
+Bounded output explicitly identifies missing or truncated diagnostics. Details remain
+scoped to checks belonging to the selected repository and revision, use server-side
+credentials and apply the public-output redaction contract. External destination links
+are references, not permission to fetch arbitrary URLs or access another provider.
+Direct Cloud Build/gcloud and build-log retrieval are outside this capability; a link-only
+failure must say that its diagnostic log is unavailable through this interface.
+
+Governing: [PR retrieval](#retrieve-pull-requests-through-the-cli),
+[server-side credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side)
+and [public-output redaction](../concepts/security.g5.md#redact-secrets-from-public-output).
+
 ### Create or update a PR from a workspace agent

 [D · g5.governance.pr.write]
@@ -486,6 +540,24 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
 - [CLI PR retrieval request and scope selection](../../chat_intent/codex-pr-user-authorship/request.md). Command and repository selection details are agent design decisions.
 - Implemented in `g5d-cli/src/pr.rs`, `routes/{workspaces,internal}.rs`, incarnation auth middleware and `workspace_pull_request_read.rs`. Compiled CLI tests cover environment, config, stable metadata and explicit selection; restricted-role API tests cover user/VM admission, repository selection, narrow App and personal credentials, redirects and failures. Live GitHub and deployed guest execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#cli-retrieval-consolidated-discovery-before-repair).

+### g5.governance.pr.checks
+
+- human-verified
+- [VM agent CI loop request](../../chat_intent/codex-pr-user-authorship/request.md#inspect-pr-checks-from-the-vm). CLI syntax, multi-repository aggregation and observation limits are agent design decisions.
+- Implemented · `workspace_pull_request_checks.rs` extends the shared authorized reader; `g5d-cli/src/{pr,pr_json,process}.rs` owns presentation and cancellable polling. API/CLI tests cover user/VM reads, selectors, all-member outcomes, permission failures, bounded enumeration and exit codes. Live GitHub and deployed VM execution remain unverified. [Checks evidence](../../chat_intent/codex-pr-user-authorship/review.md#pr-checks-final-assessment).
+
+### g5.governance.pr.checks.revision
+
+- agent
+- [CI read design](../../chat_intent/codex-pr-user-authorship/ontology-plan.md#current-amendment-pr-checks-for-the-vm-agent) derives revision checks from the user's push/check/fix loop and existing publication and approval boundaries.
+- Implemented · the shared checks owner revalidates head/base/test-merge revisions and suite/run/status snapshots; rows distinguish `headRefOid` and `commitOid`. API regressions cover expected-head mismatch, source/base changes, queued reruns, distinct Apps and test-merge failures; compiled CLI tests cover pending watch and cancellation. This is bounded observation evidence, not live-provider or merge-atomicity proof.
+
+### g5.governance.pr.checks.details
+
+- human-verified
+- [Failure reading and Cloud Build exclusion](../../chat_intent/codex-pr-user-authorship/request.md#inspect-pr-checks-from-the-vm). Bounded GitHub output and link handling are agent design decisions.
+- Implemented · the checks owner retains and redacts GitHub output, bounds annotations/text, preserves partial output on retrieval failure, and stops requests on rate limits. API/CLI regressions verify partial diagnostics, 403/429, multi-repository fanout, secret/enum separation and external-link refusal. Actual Cloud Build payloads and direct logs remain unverified/outside scope.
+
 ### g5.governance.pr.write

 - human-verified
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 89431f13..1df9856f 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -19,6 +19,7 @@ and [governance](../features/governance.g5.md); post-MVP governance retains its
 | CoordinatedRepoPublicationService / PreparedBranchCommit / ApprovalReviewService | Prepare/apply; `land*`; approval `admit`, `settle_terminal` | [Governance coordination](#publication-and-approval-coordination) |
 | CoordinatedRepoPublicationService / WorkspacePullRequestProviderLifecycleService | `list_workspace_pull_requests`, `read_github_pull_request`; public and incarnation-authenticated `GET /workspaces/:id/pull-requests/info` | [PR retrieval](../features/governance.g5.md#retrieve-pull-requests-through-the-cli) |
 | Shared create/update operation | UI and CLI create/update; public and current-incarnation adapters | [One PR flow](#one-create-or-update-pr-flow) |
+| PR read owner | Read selected provider checks and optional diagnostics through public and current-incarnation adapters | [Shared check reads](#one-provider-check-read-operation) |
 | DocService | `list_docs`, `get_doc`, `live_doc_read`, `sync_view_by_ref`, `create_explainer` | [Documents](#document-operations) |
 | OntologyService | `get_ontology_tree`, `get_node`, `get_node_dependencies`, `resolve_linkable_node_ids`, `sync_specs_for_branch` | [Semantic interpretation](#ontology-interpretation) |
 | CommentService | Anchored comment CRUD and replies; thread storage for review | [Comments](#comment-anchors-and-replies) |
@@ -64,6 +65,25 @@ Governing: [agent PR creation](../features/governance.g5.md#create-or-update-a-p
 [owner authority](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) and
 [current token authority](../concepts/security.g5.md#admit-actions-against-current-authority).

+### One provider check read operation
+
+[D · g5.architecture.pr_checks]
+
+The existing PR read owner resolves authorized workspace/provider identity and exposes
+one check-reading operation to signed-in and VM callers. Its provider adapter owns
+GitHub protocol, pagination and check/status normalization; callers receive revision,
+availability and diagnostic evidence without receiving credentials. CLI polling repeats
+this operation with current admission and bounded requests, honoring provider rate limits.
+This keeps observations separate from the shared create/update flow: reading checks
+does not save, publish, repair, rerun CI, start compute or grant merge authority. GitHub
+owns check state; no new database columns, queue or server-side monitor are introduced.
+
+Governing: [check observations](../features/governance.g5.md#inspect-provider-checks-from-a-workspace-agent),
+[revision binding](../features/governance.g5.md#bind-check-observations-to-the-requested-revision),
+[diagnostic scope](../features/governance.g5.md#read-the-failure-evidence-published-to-github),
+[current admission](../concepts/security.g5.md#admit-actions-against-current-authority)
+and [existing infrastructure](../concepts/system_design_principles.g5.md#reuse-established-infrastructure).
+
 ### Document operations

 [D · g5.architecture.semantic_surfaces]
@@ -131,6 +151,12 @@ synchronization to API maintenance through shared typed Git readers.
 - [Shared flow and manual retry requests](../../chat_intent/codex-pr-user-authorship/request.md#manual-retry-without-new-migrations). Removing pending-state persistence preserves one publication owner and the committed-work continuation.
 - Implemented in the existing shared service, provider lifecycle, bounded maintenance and UI/CLI adapters. Full-flow outcomes reuse PR events; local preflight cannot settle provider failure, and missing-provider discovery shares synchronization eligibility. Main migrations 001–054 are unchanged. API, CLI and UI checks cover manual recovery, request-scoped titles, current admission and preserved completed-provider evidence. Live provider/guest execution remains unverified. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).

+### g5.architecture.pr_checks
+
+- agent
+- [CI read design](../../chat_intent/codex-pr-user-authorship/ontology-plan.md#current-amendment-pr-checks-for-the-vm-agent) applies the shared-owner and no-new-state principles to the requested VM agent loop.
+- Implemented · `workspace_pull_request_checks.rs` reuses attached-repository selection, current user/VM admission, effective secret configuration and `RepositoryOperation::ReadPullRequestChecks`; the credential owner grants only Pull requests/Checks/Commit statuses read. API/static and existing publication regressions pass. No new database state or CI mutation. [Checks evidence](../../chat_intent/codex-pr-user-authorship/review.md#pr-checks-final-assessment).
+
 ### g5.architecture.semantic_surfaces

 - agent
```

**Relevant and governing ontology.** The following commitment set governs coding and review.

| Governing IDs and home | Obligation and owner/operation | Allowed case / counterexample; conclusion |
|---|---|---|
| `g5.governance.pr.checks`, `.revision`, `.details` ([governance](../../ontology/features/governance.g5.md#inspect-provider-checks-from-a-workspace-agent)) | Shared PR read owner returns observed checks and scoped diagnostics with revision and availability evidence. | All linked providers can be inspected; old-head green, hidden pagination loss and unavailable data cannot establish success. Implemented through the shared reader; see final review evidence. |
| `g5.governance.pr.read`, `.json` ([retrieval](../../ontology/features/governance.g5.md#retrieve-pull-requests-through-the-cli)) | Reuse selectors, authorized attached-repository resolution and validated field selection. | A current-workspace PR or explicit attached GitHub PR is readable; an arbitrary URL cannot expand access. Existing read policy inspected and preserved. |
| `g5.security.read`, `.tenant`, `.tenant.rls`, `.admission`, `.credentials.publication`, `.credentials.redaction` ([security](../../ontology/concepts/security.g5.md)) | Server admits each user/VM read, retains repository credentials and redacts public diagnostics. | Authorized durable reads do not require starting compute; stale VM credentials, cross-tenant IDs and provider links do not grant access. Preserve operation-specific read rules, not mutation-only admission. |
| `g5.governance.pr.write`, `.follow-push`, `.publish.readiness` ([publication](../../ontology/features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent)) | Existing publication owner alone saves/pushes and synchronizes PRs. | Agent publishes then reads; a checks request cannot silently repair a failed publication. No change to manual-retry behavior. |
| `g5.governance.approval`, `.approval.version`, `.merge` ([approval](../../ontology/features/governance.g5.md#approval-is-a-human-authority)) | Human approval and merge retain their own exact-version checks. | Passing observed checks support investigation; they do not approve or prove merge readiness. Preserved. |
| `g5.data.git-authority`, `.platform-authority.vm-access`, `.outcome-evidence` ([data](../../ontology/concepts/data_model.g5.md)); `g5.architecture.pr_checks` ([owner](../../ontology/software_architecture/git_governance.g5.md#one-provider-check-read-operation)) | API read owner keeps Git revision, G5 publication evidence and live provider outcomes distinct. | Provider data is read through the API; stored G5 head or successful push alone is not CI success. No new database state. |
| `g5.design.reuse`, `.minimal-mechanism`, `.adapters`, `.authority`, `g5.architecture.policy` ([principles](../../ontology/concepts/system_design_principles.g5.md)) | Extend one read owner; thin adapters share policy, retaining read/write differences. | CLI and later UI may consume one operation; no CLI-side provider credentials, duplicate check policy or server monitor. Reused owners inspected; no new state or mutation flow. |
| `g5.quality.review`, `.review.cases`, `.review.producer`, `.evidence`, `.evidence.fakes`, `.evidence.unknown` ([coding](../../ontology/concepts/coding_principles.g5.md)) | Review all read consumers, test real admission/transport boundaries with provider fakes outside the owner, and report gaps. | Exercise both valid reads and refused selectors/tokens; a mocked provider test is not live GitHub/VM proof. Validation plan below. |

#### Proposed interface and compatibility

```sh
g5d pr checks                         # current outgoing G5 PR, all linked provider PRs
g5d pr checks --watch                 # wait for observed checks to finish
g5d pr checks --details               # GitHub-published failure diagnostics
g5d pr checks --json name,state,bucket,link,repository,headRefOid,details
g5d pr checks 42 --repo OWNER/REPO     # existing explicit GitHub selection
# Guard a one-repository push/check loop against an older provider head:
g5d pr checks --watch --head "$(git rev-parse HEAD)"
```

- Reuse the existing default outgoing-PR selection, explicit G5 UUID, GitHub number/URL,
  `--repo` and workspace context. G5 selection expands every linked provider PR; `--repo`
  selects one member. Missing required publication is reported with the existing Update
  PR recovery direction; a local-only PR has no provider CI to report.
- Match [gh pr checks](https://cli.github.com/manual/gh_pr_checks) for `--watch`,
  `--interval` (default 10 seconds), `--fail-fast` and `--json FIELDS`. Validate arguments
  before calls. Bare `--json` lists fields offline. JSON is an array of checks, using the
  gh fields `bucket,completedAt,description,event,link,name,startedAt,state,workflow` where
  the fact exists. G5 adds repository/PR/revision identity and optional details; unavailable
  fields remain unknown. Watch emits progress on stderr and one final JSON array on stdout.
- `--details` and `--head SHA` are deliberate G5 extensions. Expected head applies only to
  one unambiguously selected repository; multi-repository use requires `--repo`, rather
  than comparing the same SHA with unrelated members. Reject invalid combinations locally.
  The watch keeps the expected SHA fixed, re-reads the provider head each cycle and reports
  a mismatch as pending. Without it, watch follows the current provider head. Recheck the
  head before terminal output and discard results if it changed during enumeration.
- Exits: 0 for a complete, nonempty observed set containing only pass/skipping buckets;
  1 for failures/cancellation or retrieval/authorization/incomplete-enumeration errors;
  8 for pending, an expected-head mismatch or no checks reported yet. While watching,
  wait for pending checks unless `--fail-fast` sees a failure; errors stop with actionable
  messages. Empty JSON never implies success. Missing one member cannot make the whole
  G5 PR green. Bucket spelling follows gh: pass, fail, pending, skipping, cancel.
- First version excludes `--required`, branch-name selectors, `--web`, jq/templates,
  check rerun/cancel, raw build logs and `pr view --json statusCheckRollup`. Supporting
  these later must reuse the same owner. Without a required-check inventory this reports
  observed checks only, including neutral/skipped outcomes; it cannot prove that an
  unscheduled required check will never appear or that branch protection is satisfied.

#### Owner, data and diagnostic boundaries

Extend the existing PR read capability with one operation returning per-provider identity,
live head SHA, check rows and explicit availability/completeness outcomes. The CLI formats
that result and polls it; its exit decision uses the entire selected result before JSON
projection. There is no write, background job, migration or durable cache.

Read both check runs and legacy commit status contexts, preserving provider identity and
latest-attempt/context semantics rather than arbitrarily collapsing equal names. Paginate
check enumeration with a documented finite bound; reaching the bound is an incomplete read,
not success. A PR head identifies the observation. Results associated with a different
revision must be explicitly identified and cannot satisfy the requested head silently.
A head change or rerun during watching invalidates the corresponding earlier result.

Fetch details only when requested, through GitHub endpoints for the selected check.
Expose its output title/summary/text and annotations with file/line locations; statuses
provide description/link only. Use finite response/page/byte limits with explicit
truncation and diagnostic availability. Never follow `details_url`, annotation links or
status target URLs as API destinations. GitHub content is diagnostic data, not trusted
instructions, and configured secret values must be redacted before public output.

The existing credential owner must select only authorized repository access. GitHub's
[Checks API](https://docs.github.com/en/rest/checks/runs) requires Checks read permission;
[commit statuses](https://docs.github.com/en/rest/commits/statuses) require Commit statuses
read for fine-grained credentials. Audit existing App installation/user/personal read
policies during coding and add only needed read capability. Missing permissions produce
an actionable connection/installation error, never a bot/token fallback or hidden data
loss. Provider details stay subject to workspace and repository admission on every poll.

Cloud Build checks are visible to the extent they publish data to GitHub. A failure with
only a link cannot supply compiler/test logs; report this limitation and preserve the
link. Neither direct Cloud Build access nor GitHub Actions log downloads are promised.
An agent can complete an autonomous repair only when available diagnostics suffice.
Update the existing worker PR guidance during implementation to teach publish, wait for
the expected head, inspect details, repair, and repeat, including this stopping condition.

#### Acceptance and review

1. Current workspace, explicit G5 PR and explicit attached GitHub selectors return the
   same live check facts through signed-in and current-incarnation API callers. Multiple
   repositories stay distinct, including identical check names and partial provider errors.
2. A head containing check runs plus legacy statuses includes both; reruns select current
   attempts, pagination is complete or explicitly incomplete, and an empty set is pending.
3. After pushing B while GitHub still shows A passing, `--head B` cannot return success.
   A head change while reading/watching drops A's observations; no fresh read mixes heads.
4. Queued/running, passed, failed, cancelled, neutral/skipped and unknown results produce
   honest display/JSON/exit outcomes. Watch cancellation, interval validation, fail-fast,
   rate limits and API timeouts remain bounded without server monitoring state.
5. Requested diagnostics include summaries/annotations when supplied, and explicit
   absent/truncated outcomes otherwise. A link-only Cloud Build check reports failure
   without claiming to expose its logs; external URLs are never fetched.
6. Revoked access, wrong tenant/repository, stale VM incarnation and insufficient provider
   permissions cannot leak check data or tokens. Reads cause no publication, lifecycle,
   approval, merge or CI mutation. Existing create/update behavior is unchanged.
7. Compiled CLI tests exercise JSON discovery/projection, selectors, exits and watch;
   API tests exercise the actual read/admission owner with restricted DB roles and mocked
   provider protocol. Live-provider and deployed-VM checks remain separate evidence.

Self-review used the design guide and a deliberate alternate route from the
[system invariant index](../../ontology/concepts/system_invariants.g5.md) to
[data authority/outcome evidence](../../ontology/concepts/data_model.g5.md),
[current admission](../../ontology/concepts/security.g5.md#admit-actions-against-current-authority)
and [version-bound approval](../../ontology/features/governance.g5.md#bind-human-approval-to-the-version-reviewed).
This caught two unsafe simplifications: a successful push or stale G5 pin does not prove
current provider CI success, and check reads must not inherit live mutation/compute-start
requirements. The expected-head guard, honest observed-only result and shared read owner
address those cases. Zero checks and missing provider permissions must not collapse to green.

Implementation assumptions resolved: existing read admission, repository selection and
credential/configuration owners were inspected and reused. Protocol fixtures now cover
queued reruns, pagination limits, source/base changes and explicitly labeled test-merge
checks. Deployed GitHub App grants and actual Cloud Build diagnostic payloads remain
unverified; no new credentials or direct log access are inferred. See the
[final assessment](review.md#pr-checks-final-assessment) for executed checks and limits.
The [original design handoff](reviews/pr-checks-design-handoff.md) is preserved separately.

### Previous amendment: manual retry without schema additions

The user accepts manual retry with a good error and requests removal of migrations 055/056.
This amendment supersedes all earlier pending-title/pending-refresh/error-column and
mandatory automatic-failure-retry decisions below; those sections are historical handoffs.
Main migrations 001–054 remain unchanged. No replacement queue or pending state is added.

Changed commitments: `g5.governance.pr.follow-push`, `g5.governance.pr.title`, and
`g5.architecture.pr_flow`. Scoped metadata for `g5.architecture.recovery.progress` and
`g5.design.commit-notify` records current verification. Automatic refresh after successful pushes remains; failures
identify incomplete publication and direct the owner to Update PR / `g5d pr update`.
Explicit failed titles require resubmission with `--title`; omission preserves provider
titles. Current PR identity, Git heads and existing provider outcomes are sufficient for
reconciliation; existing provider status/error and PR result surfaces own error reporting.
No new persistence is required solely to remember outstanding title intent or refresh work.

Governing obligations: `g5.governance.pr.write` keeps UI/CLI on one complete flow;
`g5.governance.publish.readiness` retains committed publication preconditions; current source,
owner, incarnation and target admission remain enforced; `g5.governance.approval.version`
and merge contracts prohibit repinning accepted merges; `g5.architecture.recovery` binds
retries to the same PR and current intent, `g5.architecture.recovery.progress` bounds
maintenance so failures do not starve other PRs, and `g5.design.commit-notify` keeps
notifications after commit. The system invariant index was checked through Git/platform
authority and version-bound approval links; no broader authority or merge redesign is needed.

Acceptance boundaries: migrations absent from the branch diff and fresh DB setup works;
create/update and successful native pushes still update the same PR; a provider failure
is visible and an explicit UI/VM retry repairs commit evidence, including an already-included
member without creating an empty PR; omitted titles preserve external edits, resubmitted
titles retry, and older overlapping titles cannot overwrite newer accepted ones. Failed,
closed, unauthorized and obsolete operations retain their existing protections. Remove
pending-field API/UI/CLI consumers and update docs/tests to the supported outcome fields.

Self-review: the smaller recovery contract is explicitly human-authorized. It preserves
idempotent provider identity and clear partial outcomes while dropping durable title
intent. Source inspection must verify the existing error surfaces and manual synchronization
policy before reuse; any necessary consequential refinement is recorded here first.
The user has authorized implementation of this amendment.

Source-inspection refinement: existing PR events already retain failure messages. Automatic
failures append a `provider_failed` event and the existing `blockingMessage` projection
shows the latest full-flow failure. Only a complete shared-flow synchronization supersedes
it; local preflight refresh cannot establish provider success. Full-flow outcome events
carry the existing source/target evidence fields and use effect-time timestamps under the
PR lock; individual provider events have no such vector. This reuses history without
adding pending state. Current-head comparison and recorded provider pins
still permit bounded background catch-up, but manual retry is the promised recovery path.
Discovery reuses the provider owner's actual candidate eligibility to find a required
missing provider row even if a local preflight already advanced evidence. Explicit
synchronization also reconciles recorded provider PRs for included members, without
creating new empty PRs. Failed attempts retain the recorded successful provider pins and
observed proof; completed recorded PRs are refused before provider publication effects. Request-scoped title effects compare against the accepted
G5 title under the PR lock; a superseded request is refused instead of applying stale text.

```diff
diff --git a/.g5/ontology/concepts/system_design_principles.g5.md b/.g5/ontology/concepts/system_design_principles.g5.md
index 3bf935a4..eaedadc0 100644
--- a/.g5/ontology/concepts/system_design_principles.g5.md
+++ b/.g5/ontology/concepts/system_design_principles.g5.md
@@ -264,12 +264,14 @@ Changes to a persistent or public contract must preserve existing valid state th
 - agent, human-verified
 - User-approved refinement in the [study's applied improvements](../../../docs/experiments/balance-study/README.md#candidate-improvements-applied-after-the-study); the [balance study](../../../docs/experiments/balance-study/README.md#were-the-two-missed-bugs-already-covered-by-ontology) exposed unfair batching and an unverified repair in an isolated candidate.
 - Partial · the PR push-refresh worker uses bounded SQL lock waits and a per-item deadline; a real held-lock regression verifies later-organization progress and retry after unlock. Other maintained recovery workers remain unassessed; the experimental candidate does not establish their state. [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). The [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings) additionally verifies held-membership admission, timeout error persistence, and scan-leader cancellation takeover. Server-side lock limits precede continuation scope admission; failure reconciliation also bounds completion reads. Stalled-filesystem and mid-scan connection-loss injection remain unexecuted.
+- PR discovery also detects required missing provider outcomes after local evidence refresh, without a pending marker. Existing lock/deadline/leadership regressions and the missing-member case pass in the [manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).

 ### g5.design.commit-notify

 - agent
 - Applies the supplied [design guidance](../../uplift/g5-lean/evidence/design-guidance.md) to [G5 module contracts](../software_architecture/boundaries.g5.md).
 - ~70% · task journals, projections and report outbox commit durable state; `ScopedDb::on_commit` (`g5-authz/src/scoped_db.rs`) runs queued effects only after a successful commit, and `notification_service.rs` and `comment_service.rs` publish through it (a rolled-back mention savepoint withdraws its notices). Other event producers have not been audited against this rule.
+- Shared PR success/failure history commits before update events; a real timeout/WebSocket/public-read regression verifies visible failure and explicit recovery in the [manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).

 ### g5.design.cleanup-proof

diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 4e0e116e..354e478e 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -218,11 +218,13 @@ observes remote publication without bypassing its auto/manual push policy. It ca
 retarget one, save dirty files, start compute or confer approval. Failed or superseded
 pushes cannot advance publication evidence; closed/merged PRs remain unchanged.

-Refresh obligations survive interruptions and failed provider synchronization, and retry
-without regressing to older source heads. Failure remains visible and cannot undo a
-successful Git push or be presented as successful provider publication. Approval remains
-bound to its reviewed version; merge pins and revalidates the refreshed commits, never
-repinning an unresolved merge operation.
+Failures identify what remains incomplete and direct the owner to retry Update PR or
+`g5d pr update` after correcting the cause. Manual retry is an acceptable recovery path;
+no separate pending-refresh or error columns are required. Existing PR identity and commit
+evidence govern retry, which must reconcile current heads rather than restore older ones.
+Failure cannot undo a successful Git push or be presented as successful provider publication.
+Approval remains bound to its reviewed version; merge pins and revalidates the refreshed
+commits, never repinning an unresolved merge operation.

 Governing: [shared publication owner](../software_architecture/git_governance.g5.md#one-create-or-update-pr-flow),
 [current authority](../concepts/security.g5.md#admit-actions-against-current-authority),
@@ -253,9 +255,10 @@ The shared create/update operation accepts an optional nonblank title, exposed a
 provider PRs, including existing PRs; provider update failures retain the existing partial
 publication reporting. Omission preserves existing titles and derives titles from commits
 for new PRs, retaining the workspace-name fallback. No new title input is required to use
-the existing UI buttons. Pending explicit title changes survive partial failure; retries
-synchronize the latest accepted title before clearing the pending effect. Completed title
-changes are not reasserted on ordinary omitted-title updates. Existing provider PRs remain
+the existing UI buttons. A failed provider title update reports that the caller must
+resubmit `--title` to retry it; no pending title is stored. Omitted-title updates never
+reassert the G5 title onto an existing provider PR. Overlapping requests cannot apply an
+older title over a newer accepted title. Existing provider PRs remain
 eligible for title synchronization when their member changes are already included in the
 target; this never authorizes a new empty provider PR.

@@ -492,9 +495,8 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
 ### g5.governance.pr.follow-push

 - human-verified
-- [Automatic refresh after push request](../../chat_intent/codex-pr-user-authorship/request.md). Existing-PR scope and durable retry are agent design details preserving explicit creation and merge authority.
-- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 056 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Contended SQL admission/error recording is bounded, and later organizations progress before the lock is released. Open source dialogs consume current query state after mutations; see [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). Dedicated advisory scan ownership, cancellation takeover, persisted timeout error plus post-commit event, and held-membership progress/retry are covered by real-database tests in the [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings). Failure reconciliation has its own deadline and scope admission installs a server lock timeout before waiting. Mid-scan connection loss and stalled-filesystem injection remain unexecuted.
-- Client PR queries read again when an update overtakes their first read and after workspace reconnect, using the shared `invalidateAndReadAgain` helper. Active-query/socket regressions verify current source/target evidence and unrelated-workspace isolation; [rebase review](../../chat_intent/codex-pr-user-authorship/review.md#rebase-91bc4742-final-verification).
+- [Manual retry and migration removal request](../../chat_intent/codex-pr-user-authorship/request.md#manual-retry-without-new-migrations) supersedes the agent-designed durable retry fields. Automatic successful-push refresh and existing authority/merge protections remain required.
+- Implemented without new migrations in the shared PR owner, provider lifecycle and existing history/error projections. Real API/DB/Git tests verify successful push refresh, visible failures through local-only refresh, manual retry, missing-provider discovery, bounded progress and unchanged authority/approval checks. UI/cache tests verify current errors and recovery. Live GitHub, deployed guests and process restart remain unexecuted. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).

 ### g5.governance.pr.target

@@ -506,8 +508,8 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
 ### g5.governance.pr.title

 - human-verified
-- [Title support follow-up](../../chat_intent/codex-pr-user-authorship/request.md); omission preserves existing titles and explicit titles reach provider PRs.
-- Implemented in shared creation, coordinated publication and provider lifecycle services, with migration 055 retaining pending title intent. Public API tests with mocked GitHub cover create/reuse/omission, failure and retry, preserved provider identity, overlapping title requests serialized on the PR row, and existing unchanged-member PRs. Compiled CLI tests cover title transport and validation. Live GitHub execution remains unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). A concurrent close refuses title acceptance and preserves terminal metadata; [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
+- [Title support and manual retry requests](../../chat_intent/codex-pr-user-authorship/request.md#manual-retry-without-new-migrations). Failed explicit titles may require resubmission, superseding durable pending title intent; omission and concurrency protections remain.
+- Implemented with request-scoped title input and PR-row serialization/comparison. API tests with mocked GitHub cover explicit resubmission, omission, external edits, overlapping/stale requests, included-member updates and terminal refusal. UI/VM retries preserve completed provider pins and observed proof; arbitrary post-advance merge import is not claimed. Compiled CLI checks pass; live GitHub remains unexecuted. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).

 ### g5.governance.pr.json

diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 8a3b0df3..89431f13 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -55,7 +55,9 @@ retains the review-workspace restriction; review-specific handback remains a sep
 Automatic [push continuation](../features/governance.g5.md#follow-successful-source-pushes-automatically)
 reuses this owner's committed-work publication stages for an existing PR, without invoking
 explicit save/push preparation. Git entry points signal accepted ref changes; this owner
-handles durable discovery/retry, current authorization and PR/provider evidence.
+handles current-head discovery, current authorization and PR/provider evidence. Failures
+use existing outcome reporting and the same explicit Update PR flow for manual retry;
+this flow adds no pending-title, pending-refresh or refresh-error database columns.

 Governing: [agent PR creation](../features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent),
 [publication readiness](../features/governance.g5.md#refuse-pr-creation-while-work-is-not-ready),
@@ -125,10 +127,9 @@ synchronization to API maintenance through shared typed Git readers.

 ### g5.architecture.pr_flow

-- human-provided
-- [One create/update flow requirement](../../chat_intent/codex-pr-user-authorship/request.md); the user explicitly requires exactly the same flow as the UI buttons.
-- Automatic push continuation shares the evidence/provider owners with explicit creation, using published refs and current ownership without VM preparation. Durable pending state, bounded provider reads, scope-reusing credential lookup and PR serialization support recovery; native/API/concurrency/migration and all three PR UI consumers are covered in the [automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Target choices, omitted-target resolution and title input share this owner; the provider lifecycle consumes durable pending title intent under a PR row lock. API/CLI/UI follow-up coverage includes failure/retry, concurrent title acceptance and target-cache refresh. [Follow-up evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Implemented in `backend/g5-api-core/src/services/workspace_pull_request_create.rs`; public and internal route adapters share the full sequence and current authority checks. Real HTTP tests, scripted native peers, mocked providers and compiled CLI/UI consumers cover reuse, refusals and partial-error recovery. Standard creation retains review restrictions. Actual active guest/native callback and live-provider execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment). The [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment) verifies queued target authority, terminal title protection, contended-worker progress and post-mutation UI freshness. The typed public-route delegation is recognized by the static authority audit, with negative caller/path calibration and shared-service admission checks. Timeout reconciliation remains in the shared owner; [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings) verifies durable error/event recovery and bounded scope-admission progress.
-- Client event/reconnect recovery and destination refresh retention reuse the existing query helpers, with shared dialog/page recovery presentation; [rebase review](../../chat_intent/codex-pr-user-authorship/review.md#rebase-91bc4742-final-verification) records active-cache and mounted-page regressions.
+- human-verified
+- [Shared flow and manual retry requests](../../chat_intent/codex-pr-user-authorship/request.md#manual-retry-without-new-migrations). Removing pending-state persistence preserves one publication owner and the committed-work continuation.
+- Implemented in the existing shared service, provider lifecycle, bounded maintenance and UI/CLI adapters. Full-flow outcomes reuse PR events; local preflight cannot settle provider failure, and missing-provider discovery shares synchronization eligibility. Main migrations 001–054 are unchanged. API, CLI and UI checks cover manual recovery, request-scoped titles, current admission and preserved completed-provider evidence. Live provider/guest execution remains unverified. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).

 ### g5.architecture.semantic_surfaces

```

### Latest rebase reconciliation with main 20df7d80

Main adds `g5.governance.approval.request` and `.requirements`: addressed humans
with current read access can settle their pending request without workspace
ownership; organization administrators manage requirements without gaining content
or compute authority. Preserve these grants separately from PR publication's owner
authority, read-only PR checks and version-bound approval. Requestless decisions
retain main's legacy owner guard. No PR contract changes are necessary.

The only textual conflict is the `g5.architecture.governance_owner` implementation
footer. Remove its obsolete claim that nonowner approval is excluded, as main does,
while preserving this PR's reader/publication evidence. The adjacent
`g5.architecture.pr_flow` footer now describes the unchanged migration directory
without a stale version range. Contract bodies and all four implementation patches
remain unchanged; main's approval audit changes compose with the PR guard audit.
[Incremental evidence](review.md#rebase-20df7d80-approval-authority) covers the
combined approval/PR boundaries. The four-commit, migration-free history is retained.

### Earlier rebase reconciliation with main 93d7c39e

The requested rebase preserves main's independent review-edit decisions and its
source-lock-before-incarnation-fence ordering, Claude background/End behavior,
explicit-versus-default execution deadlines, and provider CLI/model upgrades.
These govern separate outcomes from the PR read/publication and workspace CLI
contracts; their combined ontology has no conflicting commitment. The shared
runtime environment changes remain independent of provider lifecycle policy.

Main's `055_review_suggestion_edits.sql` and checksum remain unchanged. The PR's
manual-retry design still adds no schema state. At the user's request, the commits
that introduced and later removed retry-state migrations are squashed together;
every surviving PR commit now has exactly main's migration directory. This changes
history only, preserving the final implementation and existing review evidence.
The incremental self-review and checks are recorded in
[rebase evidence](review.md#rebase-93d7c39e-and-migration-history).

### Earlier rebase reconciliation with main 91bc4742

Main adds comment/review-thread authority and attribution, principal-bound comment
requests, retained page content through transient refresh failures, and after-commit
notification hooks. Preserve these alongside shared PR publication and committed-push
refresh. Review suggestions remain a separate hand-back that moves no Git refs; ordinary
PR creation retains its review-workspace restriction. Main explicitly defers principal
binding of non-comment actions; this rebase preserves current server-side PR authority
without widening that scope. Keep both sets of WebSocket invalidations and main's
composer identity/refresh status around the owner-visible PR destination retry.

Main owns immutable migrations 001–054. Branch pending-title and push-refresh migrations
are now 055/056 with unchanged SQL and SHA384 values. The populated upgrade test starts
at 001–055, applies 056 and replays under a non-BYPASSRLS table owner. Preserve main's
historical migration-owner contexts. Governing meanings are compatible; ontology was
reconciled before dependent code. Current evidence references for
`g5.governance.pr.title` and `g5.governance.pr.follow-push` use 055/056. Other ontology
additions and updated metadata from main remain intact; historical review receipts remain
in `review.md`. No new behavioral commitment or authority is introduced.

The rebase review also applies main's `g5.workspace.page.refresh` to loaded PR
destinations: retain their last value through transient errors, keep drafts/selection,
and show refresh status/retry; initial errors and access refusals remain unavailable.
For `g5.governance.pr.follow-push` and `g5.design.commit-notify`, the existing client
`invalidateAndReadAgain` owner refreshes PR queries after an update or workspace reconnect,
including a first read still outstanding. No new persistence or publication owner is added.

Scoped assessment changes for the repaired consumers (governing meaning unchanged):

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 59a6b1ff..4e0e116e 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -496 +496,2 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 053 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Contended SQL admission/error recording is bounded, and later organizations progress before the lock is released. Open source dialogs consume current query state after mutations; see [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). Dedicated advisory scan ownership, cancellation takeover, persisted timeout error plus post-commit event, and held-membership progress/retry are covered by real-database tests in the [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings). Failure reconciliation has its own deadline and scope admission installs a server lock timeout before waiting. Mid-scan connection loss and stalled-filesystem injection remain unexecuted.
+- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 056 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Contended SQL admission/error recording is bounded, and later organizations progress before the lock is released. Open source dialogs consume current query state after mutations; see [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). Dedicated advisory scan ownership, cancellation takeover, persisted timeout error plus post-commit event, and held-membership progress/retry are covered by real-database tests in the [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings). Failure reconciliation has its own deadline and scope admission installs a server lock timeout before waiting. Mid-scan connection loss and stalled-filesystem injection remain unexecuted.
+- Client PR queries read again when an update overtakes their first read and after workspace reconnect, using the shared `invalidateAndReadAgain` helper. Active-query/socket regressions verify current source/target evidence and unrelated-workspace isolation; [rebase review](../../chat_intent/codex-pr-user-authorship/review.md#rebase-91bc4742-final-verification).
@@ -502,0 +504 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
+- Loaded destinations survive transient refresh failure through `toRefreshingState`; both PR dialogs keep their draft/selection and show retry inside the active dialog. Initial failures and 403/404 refusals remain unavailable; mounted page/hook tests also verify retry and actual target removal ([rebase review](../../chat_intent/codex-pr-user-authorship/review.md#rebase-91bc4742-final-verification)).
@@ -508 +510 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented in shared creation, coordinated publication and provider lifecycle services, with migration 052 retaining pending title intent. Public API tests with mocked GitHub cover create/reuse/omission, failure and retry, preserved provider identity, overlapping title requests serialized on the PR row, and existing unchanged-member PRs. Compiled CLI tests cover title transport and validation. Live GitHub execution remains unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). A concurrent close refuses title acceptance and preserves terminal metadata; [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
+- Implemented in shared creation, coordinated publication and provider lifecycle services, with migration 055 retaining pending title intent. Public API tests with mocked GitHub cover create/reuse/omission, failure and retry, preserved provider identity, overlapping title requests serialized on the PR row, and existing unchanged-member PRs. Compiled CLI tests cover title transport and validation. Live GitHub execution remains unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). A concurrent close refuses title acceptance and preserves terminal metadata; [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index be5d52c0..06c881cc 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -617,0 +618 @@ Implicit publication follows [provider transport](../../../docs/data-model-clean
+- PR destinations use the same retained-refresh state: temporary failures keep Prepare to Ship descriptions and explicit Create PR destinations; shared status/retry stays inside the active dialog. Mounted page/hook regressions cover retention, retry, refusals and a removed target; [rebase review](../../chat_intent/codex-pr-user-authorship/review.md#rebase-91bc4742-final-verification).
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 2d6b8a96..8a3b0df3 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -130,0 +131 @@ synchronization to API maintenance through shared typed Git readers.
+- Client event/reconnect recovery and destination refresh retention reuse the existing query helpers, with shared dialog/page recovery presentation; [rebase review](../../chat_intent/codex-pr-user-authorship/review.md#rebase-91bc4742-final-verification) records active-cache and mounted-page regressions.
```

### Earlier rebase reconciliation with main 72d2b4de

The user requested checking main's new migrations and rebasing when necessary. Main
owns 046–051; branch pending-title and push-refresh migrations are 052/053 with unchanged
SQL and SHA384 values. The populated upgrade regression starts from 001–052 and applies
053, then repeats the production runner under a non-BYPASSRLS table owner. Preserve main's
checksum-qualified historical owner execution for its immutable 047/048 migrations.

Main's selected-file Save/acceptance checkpoint, current VM identity, preserved unrelated
Git state, inline tracking actions and proven divergence completion remain governing.
They compose with this branch's shared PR publication and committed-push continuation:
selected-file pushes can update an existing PR, while divergence resolution still obeys
each repository's auto/manual provider push policy. No second PR publication flow, broader
permission, or new automatic commit trigger is introduced. Preserve main's design tokens
and branch's readable merge-active message, and its relocated experiment references plus
both branches' scoped evidence. No incompatible intent was found before resolving code.

Ontology changes in this reconciliation only update migration evidence references in
`g5.governance.pr.follow-push` and `g5.governance.pr.title`; governing meaning is unchanged.
Historical execution receipts stay in `review.md`; current references follow the latest reconciliation above.

### CI and review follow-up

The user requests fixing CI and review comments on PR #268. Existing PR publication,
current-authority, visible-failure, progress and shared-flow commitments remain governing.
Both failed Cloud Build jobs report the same stale static route-auth audit: it must
recognize and verify typed delegation to the shared service, without adding a second PR
flow or weakening the guard. Target-loading failures must remain visible with retry in
all workspace layouts before a PR action can be opened.

For recovery, the scan loop holds one PostgreSQL session advisory lock on a dedicated
connection across its ticks; followers skip discovery/preflight and retry leadership.
Connection release/cancellation permits takeover; per-PR current-authority and row locks
remain authoritative for effects. Move the attempt deadline inside the shared refresh
owner so cancellation becomes a recorded, sanitized pending error and committed update
notification. Error recording must recheck current source ownership and terminal/completed
PR state and avoid overwriting a newer successful refresh. Its separate five-second
budget includes scope admission and completion reads, so failed recording cannot starve
later PRs; durable open-PR inventory still provides retry. Install the transaction-local
SQL lock deadline before scope-authority admission so cancelled queries also release
their pooled connections. The destination error banner
uses owner eligibility, including stopped workspaces, rather than live-edit eligibility.
This is a scoped implementation refinement of existing recovery and notification rules;
no publication permission or product flow changes. Detailed findings and checks remain in
`review.md`; deleted legacy profile size guidance is obsolete.

Scoped ontology changes only update evidence assessments for `g5.governance.pr.follow-push`,
`g5.governance.pr.target`, `g5.architecture.pr_flow`, and
`g5.architecture.recovery.progress`; their governing meaning is unchanged.

```diff
diff --git a/.g5/ontology/concepts/system_design_principles.g5.md b/.g5/ontology/concepts/system_design_principles.g5.md
index de6c5933..fb29b303 100644
--- a/.g5/ontology/concepts/system_design_principles.g5.md
+++ b/.g5/ontology/concepts/system_design_principles.g5.md
@@ -266 +266 @@ Changes to a persistent or public contract must preserve existing valid state th
-- Partial · the PR push-refresh worker uses bounded SQL lock waits and a per-item deadline; a real held-lock regression verifies later-organization progress and retry after unlock. Other maintained recovery workers remain unassessed; the experimental candidate does not establish their state. [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
+- Partial · the PR push-refresh worker uses bounded SQL lock waits and a per-item deadline; a real held-lock regression verifies later-organization progress and retry after unlock. Other maintained recovery workers remain unassessed; the experimental candidate does not establish their state. [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). The [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings) additionally verifies held-membership admission, timeout error persistence, and scan-leader cancellation takeover. Server-side lock limits precede continuation scope admission; failure reconciliation also bounds completion reads. Stalled-filesystem and mid-scan connection-loss injection remain unexecuted.
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 03f7777c..6a4af703 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -373 +373 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 056 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Contended SQL admission/error recording is bounded, and later organizations progress before the lock is released. Open source dialogs consume current query state after mutations; see [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
+- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 056 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Contended SQL admission/error recording is bounded, and later organizations progress before the lock is released. Open source dialogs consume current query state after mutations; see [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). Dedicated advisory scan ownership, cancellation takeover, persisted timeout error plus post-commit event, and held-membership progress/retry are covered by real-database tests in the [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings). Failure reconciliation has its own deadline and scope admission installs a server lock timeout before waiting. Mid-scan connection loss and stalled-filesystem injection remain unexecuted.
@@ -379 +379 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented by `workspace_pull_request_create.rs` and `useWorkspacePullRequestTargets`; API tests cover public/VM defaults, ordered readable parent/Base choices, invalid destinations and access loss without retargeting. A mounted hook test verifies choices refresh after workspace deletion. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Revocation during queued UI/VM local and provider phases is covered in the [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
+- Implemented by `workspace_pull_request_create.rs` and `useWorkspacePullRequestTargets`; API tests cover public/VM defaults, ordered readable parent/Base choices, invalid destinations and access loss without retargeting. A mounted hook test verifies choices refresh after workspace deletion. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Revocation during queued UI/VM local and provider phases is covered in the [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment). Running and stopped owners see initial destination-query errors and retry the same server query without starting compute; mounted UI tests verify restored Create PR availability ([CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings)).
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 0ac68717..88999cdb 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -112 +112 @@ synchronization to API maintenance through shared typed Git readers.
-- Automatic push continuation shares the evidence/provider owners with explicit creation, using published refs and current ownership without VM preparation. Durable pending state, bounded provider reads, scope-reusing credential lookup and PR serialization support recovery; native/API/concurrency/migration and all three PR UI consumers are covered in the [automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Target choices, omitted-target resolution and title input share this owner; the provider lifecycle consumes durable pending title intent under a PR row lock. API/CLI/UI follow-up coverage includes failure/retry, concurrent title acceptance and target-cache refresh. [Follow-up evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Implemented in `backend/g5-api-core/src/services/workspace_pull_request_create.rs`; public and internal route adapters share the full sequence and current authority checks. Real HTTP tests, scripted native peers, mocked providers and compiled CLI/UI consumers cover reuse, refusals and partial-error recovery. Standard creation retains review restrictions. Actual active guest/native callback and live-provider execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment). The [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment) verifies queued target authority, terminal title protection, contended-worker progress and post-mutation UI freshness.
+- Automatic push continuation shares the evidence/provider owners with explicit creation, using published refs and current ownership without VM preparation. Durable pending state, bounded provider reads, scope-reusing credential lookup and PR serialization support recovery; native/API/concurrency/migration and all three PR UI consumers are covered in the [automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Target choices, omitted-target resolution and title input share this owner; the provider lifecycle consumes durable pending title intent under a PR row lock. API/CLI/UI follow-up coverage includes failure/retry, concurrent title acceptance and target-cache refresh. [Follow-up evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Implemented in `backend/g5-api-core/src/services/workspace_pull_request_create.rs`; public and internal route adapters share the full sequence and current authority checks. Real HTTP tests, scripted native peers, mocked providers and compiled CLI/UI consumers cover reuse, refusals and partial-error recovery. Standard creation retains review restrictions. Actual active guest/native callback and live-provider execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment). The [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment) verifies queued target authority, terminal title protection, contended-worker progress and post-mutation UI freshness. The typed public-route delegation is recognized by the static authority audit, with negative caller/path calibration and shared-service admission checks. Timeout reconciliation remains in the shared owner; [CI follow-up](../../chat_intent/codex-pr-user-authorship/review.md#ci-and-code-review-follow-up--initial-findings) verifies durable error/event recovery and bounded scope-admission progress.
```

### Earlier rebase reconciliation with main 967bf34c

The user requested rebasing this branch onto main. Preserve the PR intent below together
with main's tenant/liveness fences, Base/Home presentation and provider lifecycle/limit
contracts. `g5.harness.provided` now governs delivery: both providers receive root
`AGENTS.md` and exactly four ontology skills. Conditional workspace PR guidance lives
in that shared file; command help and `docs/g5d-pr.md` own details. Deleted profiles,
slash commands and their skills remain deleted, with no extra runtime prompt path.
The original profile-based implementation handoff remains in the review history.

That earlier main owned migration versions 041–045, so the branch migrations moved
from 041/042 to 046/047 at that point. The latest reconciliation above supersedes those
numbers with 055/056; governing PR behavior is unchanged. Preserve main's additional metadata
about harness readiness and the remaining review-round tools. Verification and the
reconciliation evidence are recorded in the existing review artifact.

The Home issue-search consumer added on main now enters the same Account connection,
retaining its assignee/query/State return destination. The settings block's external-issue
wording includes this consumer; no grant or authorization policy changes. The real Claude
harness fixture owns PR-guidance verification because fake-provider turns intentionally
skip installation. These integration repairs preserve the prior design.

Rebase repair ontology delta (current governing text and paired metadata):

```diff
diff --git a/.g5/ontology/features/ontology_and_harness.g5.md b/.g5/ontology/features/ontology_and_harness.g5.md
index 3783f582..4ec47436 100644
--- a/.g5/ontology/features/ontology_and_harness.g5.md
+++ b/.g5/ontology/features/ontology_and_harness.g5.md
@@ -336 +336 @@ two with which the owner's agent reads and decides a [review round](governance.g
-- Implemented for platform-launched provider turns · `backend/g5d-agent-turn/src/harness.rs` lays the packaged harness out per provider and `runtime.rs` adds it to each launch; `infra/docker/Dockerfile.g5d{,-prebuilt}`, `infra/packer/g5-worker/scripts/30-build-g5d.sh` and `scripts/stage-local-worker-runtime.sh` package it from the repository root; `backend/g5d-cli/src/mcp.rs` serves the review-round tools, registered for Codex only; fake-provider turns read neither. Gaps: Claude Code started from the workspace terminal does not read the harness, while Codex there does once a G5 turn has installed it; Claude Code has no review-round tools; host readiness does not check the harness, so a missing one fails each turn; the instruction links do not resolve from the Codex home. Verified with fixture CLIs and offline probes of local Claude Code 2.1.245 and Codex 0.140.0; the pinned CLIs and live VMs are unexecuted.
+- Implemented for platform-launched provider turns · `backend/g5d-agent-turn/src/harness.rs` lays the packaged harness out per provider and `runtime.rs` adds it to each launch; `infra/docker/Dockerfile.g5d{,-prebuilt}`, `infra/packer/g5-worker/scripts/30-build-g5d.sh` and `scripts/stage-local-worker-runtime.sh` package it from the repository root; `backend/g5d-cli/src/mcp.rs` serves the review-round tools, registered for Codex only; fake-provider turns read neither. Conditional workspace PR guidance in root `AGENTS.md` reaches both providers through this same path; scripted launch checks verify `g5d pr` on PATH with workspace context. Gaps: Claude Code started from the workspace terminal does not read the harness, while Codex there does once a G5 turn has installed it; Claude Code has no review-round tools; host readiness does not check the harness, so a missing one fails each turn; the instruction links do not resolve from the Codex home. Verified with fixture CLIs and offline probes of local Claude Code 2.1.245 and Codex 0.140.0; the pinned CLIs and live VMs are unexecuted.
diff --git a/.g5/ontology/features/settings_and_usage.g5.md b/.g5/ontology/features/settings_and_usage.g5.md
index 7b421bd7..a4875d69 100644
--- a/.g5/ontology/features/settings_and_usage.g5.md
+++ b/.g5/ontology/features/settings_and_usage.g5.md
@@ -87 +87 @@ continue without connecting. Account settings owns connected-account status, con
-reconnection and disconnection. PR and issue-comment surfaces direct users to this
+reconnection and disconnection. PR and external-issue surfaces direct users to this
@@ -155 +155 @@ and external issue authorization retain their operation-specific checks.
-- Implemented in `GithubConnectionCard`, `GithubConnectionPage`, `UserSettingsPanel`, `LoginPage` and `useGithubConnection`, reusing existing user-grant endpoints. Frontend tests verify optional login connection, nested return paths, failures, disconnect and PR/comment recovery. Live browser OAuth remains unexecuted. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md).
+- Implemented in `GithubConnectionCard`, `GithubConnectionPage`, `UserSettingsPanel`, `LoginPage` and `useGithubConnection`, reusing existing user-grant endpoints. `HomeIssueSearch` joins PR/comment recovery through the account page, preserving assignee, terms and State in its return destination. Frontend tests verify optional login connection, nested return paths, failures, disconnect and these recovery consumers. Live browser OAuth remains unexecuted. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md).
diff --git a/.g5/ontology/software_architecture/runtime.g5.md b/.g5/ontology/software_architecture/runtime.g5.md
index 8c9d43e2..de5f33f4 100644
--- a/.g5/ontology/software_architecture/runtime.g5.md
+++ b/.g5/ontology/software_architecture/runtime.g5.md
@@ -295 +295 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
-- Shared environment builder supplies dispatcher terminals and agent.turn. A real PTY/CLI with mock API verifies inferred workspace/auth, read/create, actionable write failure and reserved-token rejection. Existing server authority and publication flow are unchanged. Restricted Codex shell networking remains intentionally unavailable under the governing policy; real model choice, live GitHub and deployed guest checks remain unexecuted. [Review](../../chat_intent/codex-pr-user-authorship/review.md#workspace-cli-usability--final-assessment).
+- Shared environment builder supplies dispatcher terminals and agent.turn; the shared root harness directs both providers to `g5d pr`, verified through their native harness launch fixtures. A real PTY/CLI with mock API verifies inferred workspace/auth, read/create, actionable write failure and reserved-token rejection. Existing server authority and publication flow are unchanged. Restricted Codex shell networking remains intentionally unavailable under the governing policy; real model choice, live GitHub and deployed guest checks remain unexecuted. [Review](../../chat_intent/codex-pr-user-authorship/review.md#workspace-cli-usability--final-assessment).
```


**Ontology changes:** Compared with branch base `main`, add
`g5.governance.publish.github-author`, `g5.governance.pr.read`, `g5.governance.pr.write`,
`g5.governance.pr.target`, `g5.governance.pr.title`, `g5.governance.pr.json` and assess
affected publication/readiness metadata in
[governance](../../ontology/features/governance.g5.md#attribute-github-pr-creation-to-the-initiating-user).

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 1ab30406..4c89e0d3 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -106,6 +106,108 @@ An ordinary review composes checked fork, reviewer-owned child start, conversati

 Explicit Finish stops the review child while dirty or unpushed work remains there, without introducing a sealed-result workflow.

+### Retrieve pull requests through the CLI
+
+[D · g5.governance.pr.read]
+
+`g5d pr` lists and views G5 workspace pull requests using the current workspace by
+default, and retrieves GitHub PR information by number or URL. Explicit provider
+selectors must identify a repository attached to the authorized workspace; ambiguous
+numbers require repository selection. Provide readable and JSON output, distinguishing
+stored G5 state from live provider information and reporting unavailable data honestly.
+Retrieval never creates, publishes, merges or starts compute.
+
+Governing: [workspace read authority](../concepts/security.g5.md#read-permission-is-explicit),
+[current token authority](../concepts/security.g5.md#admit-actions-against-current-authority)
+and [server-side provider credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side).
+
+### Create or update a PR from a workspace agent
+
+[D · g5.governance.pr.write]
+
+`g5d pr create` and `g5d pr update` invoke the same create-or-update operation as the UI
+buttons, with optional target, title and description inputs. Both names preserve
+existing PR reuse. The current workspace is the source; a VM token may act only for its
+current owner and incarnation, with target read access and the same source restrictions
+as the UI. Server-side credentials retain [acting-user authorship](#attribute-github-pr-creation-to-the-initiating-user).
+
+The command includes the UI's save/push preparation and reports the resulting G5 PR and
+provider outcomes without presenting partial provider failure as complete publication.
+It supplies no merge, approval or implicit compute-start authority. [Current admission](../concepts/security.g5.md#admit-actions-against-current-authority),
+[typed service identity](../concepts/security.g5.md#keep-human-and-machine-identities-distinct)
+and [one shared flow](../software_architecture/git_governance.g5.md#one-create-or-update-pr-flow) govern every entry point.
+
+### Follow successful source pushes automatically
+
+[D · g5.governance.pr.follow-push]
+
+An existing open G5 PR follows successful pushes of its source workspace branches without
+requiring a manual Update PR. The publication owner refreshes committed source/target
+evidence and synchronizes provider PRs using current owner authority and target access.
+This continuation uses published branch refs, never a retained dirty snapshot, and
+observes remote publication without bypassing its auto/manual push policy. It cannot create a new G5 PR,
+retarget one, save dirty files, start compute or confer approval. Failed or superseded
+pushes cannot advance publication evidence; closed/merged PRs remain unchanged.
+
+Refresh obligations survive interruptions and failed provider synchronization, and retry
+without regressing to older source heads. Failure remains visible and cannot undo a
+successful Git push or be presented as successful provider publication. Approval remains
+bound to its reviewed version; merge pins and revalidates the refreshed commits, never
+repinning an unresolved merge operation.
+
+Governing: [shared publication owner](../software_architecture/git_governance.g5.md#one-create-or-update-pr-flow),
+[current authority](../concepts/security.g5.md#admit-actions-against-current-authority),
+[version-bound approval](#bind-human-approval-to-the-version-reviewed),
+[recoverable merge](#merge-is-a-recoverable-application-of-pinned-intent) and
+[recoverable progress](../concepts/system_design_principles.g5.md#keep-recovery-work-discoverable-and-progressing).
+
+### Infer the PR destination consistently
+
+[D · g5.governance.pr.target]
+
+The shared PR owner supplies the UI's target choices and resolves an omitted target:
+prefer the readable, live direct parent, otherwise the first readable, live Base workspace
+in name order with ID as a stable tie-breaker, within the source organization and excluding
+the source itself. No eligible target is an actionable error before save/push. Explicit
+targets remain supported under existing authorization. Resolve once per operation and
+recheck access to that target at effect boundaries; never silently retarget after admission.
+
+Governing: [read authority](../concepts/security.g5.md#read-permission-is-explicit) and
+[one shared flow](../software_architecture/git_governance.g5.md#one-create-or-update-pr-flow).
+
+### Accept an explicit PR title
+
+[D · g5.governance.pr.title]
+
+The shared create/update operation accepts an optional nonblank title, exposed as
+`-t/--title` by the CLI. An explicit title becomes the G5 title and the title of its
+provider PRs, including existing PRs; provider update failures retain the existing partial
+publication reporting. Omission preserves existing titles and derives titles from commits
+for new PRs, retaining the workspace-name fallback. No new title input is required to use
+the existing UI buttons. Pending explicit title changes survive partial failure; retries
+synchronize the latest accepted title before clearing the pending effect. Completed title
+changes are not reasserted on ordinary omitted-title updates. Existing provider PRs remain
+eligible for title synchronization when their member changes are already included in the
+target; this never authorizes a new empty provider PR.
+
+### Select PR JSON fields using familiar syntax
+
+[D · g5.governance.pr.json]
+
+PR commands accept `--json FIELDS` with comma-separated supported fields, returning
+only those fields as an object for `view`/`create`/`update` and arrays for workspace `list`/`status`.
+Bare `--json` lists available fields without authentication or network access; invalid
+fields fail before retrieval or mutation. Selected fields use GitHub-compatible names and values
+where the underlying fact exists. Unavailable facts remain unknown; a multi-repository
+G5 PR must not arbitrarily choose one provider PR. Workspace-only fields remain available
+as G5 extensions. GitHub `id` is its node ID (not its PR number or numeric database ID);
+G5 `id` remains the workspace PR UUID. Other commands retain their boolean `--json` behavior.
+
+This is field-selection compatibility; workspace `status` remains a workspace array,
+not GitHub's personal/review-request grouping. jq/template formatting and provider fields
+outside the read surface are not implied. [Retrieval authority and provenance](#retrieve-pull-requests-through-the-cli)
+still apply; selected output may omit provenance when the caller does not request it.
+
 ### Approval is a human authority

 [D · g5.governance.approval]
@@ -128,9 +230,15 @@ Publish existing pushed branch OIDs rather than dirty files, flush snapshots or

 [D · g5.governance.publish.readiness]

-If `workspace.changes get` reports dirty or unpushed work, refuse PR creation with a clear not-ready result directing the agent to finish its checkpoint/push. An owner may use retained Push for existing local commits; do not publish an older OID to bypass this gate.
+Retained UI/CLI Create/Update PR explicitly saves and pushes pending source work before
+capturing evidence; failure stops PR creation and preserves completed commits for recovery.
+Never publish an older OID to bypass failed preparation. This is the explicit preparation
+of the [shared create/update operation](#create-or-update-a-pr-from-a-workspace-agent), not
+an implicit commit by the lower-level [publication owner](#publish-existing-work-under-the-actors-provider-authority).

-This follows the [publication contract](#publish-existing-work-under-the-actors-provider-authority) within the existing later P6 scope.
+The later P6 readiness policy requires a clear not-ready result directing the agent to
+finish its checkpoint/push when `workspace.changes get` reports dirty or unpushed work;
+retained Push may publish existing local commits.

 ### Publish under the acting human’s provider authority

@@ -138,6 +246,22 @@ This follows the [publication contract](#publish-existing-work-under-the-actors-

 Resolve the acting user’s provider token server-side and respect provider branch protection.

+### Attribute GitHub PR creation to the initiating user
+
+[D · g5.governance.publish.github-author]
+
+Create new GitHub PRs under the initiating G5 user's connected GitHub identity. For an
+organization using the GitHub App, require that user's App authorization and current
+repository access, preserving the binding's repository and permission restrictions.
+Missing, expired or revoked authorization requires connection or reconnection; never
+substitute the installation bot or another person's token. Organizations without an
+App binding retain personal SCM authentication. Reusing an existing PR preserves its
+author; branch publication and other App operations retain their own credential policy.
+
+Governing: [server-side publication credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side),
+[current action authority](../concepts/security.g5.md#admit-actions-against-current-authority)
+and [PR readiness](#refuse-pr-creation-while-work-is-not-ready) still apply.
+
 ### Prove initial external publication

 [D · g5.governance.publish.proof]
@@ -230,6 +354,42 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
 - Agent design [P04](../../uplift/g5-lean/investigations/product-operations.md#p04--review-workspaces-are-a-consequential-later-addition); preserves [H081](../../uplift/g5-lean/candidates/human-decisions.md).
 - ~85% · workspace_reviews finish; stop failure and retained-child UI paths unexecuted.

+### g5.governance.pr.read
+
+- human-verified
+- [CLI PR retrieval request and scope selection](../../chat_intent/codex-pr-user-authorship/request.md). Command and repository selection details are agent design decisions.
+- Implemented in `g5d-cli/src/pr.rs`, `routes/{workspaces,internal}.rs`, incarnation auth middleware and `workspace_pull_request_read.rs`. Compiled CLI tests cover environment, config, stable metadata and explicit selection; restricted-role API tests cover user/VM admission, repository selection, narrow App and personal credentials, redirects and failures. Live GitHub and deployed guest execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#cli-retrieval-consolidated-discovery-before-repair).
+
+### g5.governance.pr.write
+
+- human-verified
+- [Shared CLI/UI PR flow request](../../chat_intent/codex-pr-user-authorship/request.md). Target/title follow-up supersedes the initial explicit-target requirement; CLI aliases and result presentation are agent interface decisions.
+- Implemented by `workspace_pull_request_create.rs`, thin public/internal routes, runtime fencing and `g5d-cli/src/pr.rs`. Public/VM HTTP tests verify reuse, current source/target authority, save/push ordering and explicit partial provider errors; compiled CLI and UI tests verify projection, recovery and refreshed PR lists. Actual active-agent/native Git callbacks, live GitHub and deployed workers remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment).
+
+### g5.governance.pr.follow-push
+
+- human-verified
+- [Automatic refresh after push request](../../chat_intent/codex-pr-user-authorship/request.md). Existing-PR scope and durable retry are agent design details preserving explicit creation and merge authority.
+- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 056 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment).
+
+### g5.governance.pr.target
+
+- human-verified
+- [Implicit destination follow-up](../../chat_intent/codex-pr-user-authorship/request.md); stable tie-breaking and access filtering are agent decisions.
+- Implemented by `workspace_pull_request_create.rs` and `useWorkspacePullRequestTargets`; API tests cover public/VM defaults, ordered readable parent/Base choices, invalid destinations and access loss without retargeting. A mounted hook test verifies choices refresh after workspace deletion. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment).
+
+### g5.governance.pr.title
+
+- human-verified
+- [Title support follow-up](../../chat_intent/codex-pr-user-authorship/request.md); omission preserves existing titles and explicit titles reach provider PRs.
+- Implemented in shared creation, coordinated publication and provider lifecycle services, with migration 055 retaining pending title intent. Public API tests with mocked GitHub cover create/reuse/omission, failure and retry, preserved provider identity, overlapping title requests serialized on the PR row, and existing unchanged-member PRs. Compiled CLI tests cover title transport and validation. Live GitHub execution remains unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment).
+
+### g5.governance.pr.json
+
+- human-verified
+- [JSON compatibility follow-up](../../chat_intent/codex-pr-user-authorship/request.md). Field validation, bounded support and compatibility exceptions are agent design decisions.
+- GitHub node ID support is implemented in `workspace_pull_request_read.rs` and `pr_json.rs`; numeric and URL selectors are covered by read API and compiled CLI tests. [Follow-up evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Existing JSON projection is implemented in `g5d-cli/src/{lib,pr,pr_json,dispatch,process}.rs`. Field selection, no-context discovery, explicit empty/unknown rejection, comment demand, mergeability/unknown values and existing boolean flags are covered by 280 CLI units and 59 compiled integration tests. Three independent reviews consolidated; the explicit-empty defect is repaired. Create/update object projection, pre-mutation validation, body inputs and incomplete-provider errors are covered by the extended ten-case compiled PR suite. Live GitHub and deployed guest execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#json-compatibility-final-assessment).
+
 ### g5.governance.approval

 - agent
@@ -250,15 +410,21 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.

 ### g5.governance.publish.readiness

-- agent
-- From [C140](../../uplift/g5-lean/candidates/contracts.md), [V9 PR creation](../../../docs/lean-backend-design-v9.md#102-pull-requests).
-- Unassessed · PR creation and workspace-changes refusal paths were not inspected in this documentary repair; P6 scope remains deferred.
+- human-verified
+- [Shared UI/CLI flow request](../../chat_intent/codex-pr-user-authorship/request.md) explicitly preserves retained button behavior, including save/push preparation described before the request. Later P6 lineage: [C140](../../uplift/g5-lean/candidates/contracts.md), [V9 PR creation](../../../docs/lean-backend-design-v9.md#102-pull-requests).
+- Shared UI/VM route tests with scripted runtime peers cover save/push ordering, fresh evidence, dirty stopped snapshots, failed preparation, replaced incarnation and retry. Completed source commits survive later target-access refusal. Actual native Git execution from an active guest agent, full P6 readiness and broader lease recovery remain unverified. [Write review](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment).

 ### g5.governance.publish.actor

 - agent
 - From [C048/C140](../../uplift/g5-lean/candidates/contracts.md).
-- ~80% · publication credential resolution and provider adapter; external provider permission matrix unexecuted.
+- New App PR creation uses the acting user grant through `RepositoryCredentials`; non-App personal SCM and operation-specific installation credentials are preserved. Credential and PR lifecycle integration tests pass; external provider permission matrix remains unexecuted.
+
+### g5.governance.publish.github-author
+
+- human-verified
+- [PR authorship request](../../chat_intent/codex-pr-user-authorship/request.md); user requested implementation of creation under their connected GitHub identity. Failure and compatibility rules are agent design decisions.
+- Implemented in `auth/repository_credentials.rs`, `services/github_user_grants.rs`, `services/github_app.rs` and `workspace_pull_request_provider_lifecycle.rs`. Restricted-role integration tests verify the actor token, refresh/refusal, binding identity, persisted connection recovery and existing-PR reuse; live GitHub authorship and the external permission matrix remain unexecuted. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md).

 ### g5.governance.publish.proof

diff --git a/.g5/ontology/features/settings_and_usage.g5.md b/.g5/ontology/features/settings_and_usage.g5.md
index 56266faa..7b421bd7 100644
--- a/.g5/ontology/features/settings_and_usage.g5.md
+++ b/.g5/ontology/features/settings_and_usage.g5.md
@@ -77,6 +77,21 @@ Governing concepts: [Durable platform records](../concepts/data_model.g5.md#the-

 Accounting follows the [usage evidence rules](agents.g5.md#usage-reports-preserve-unknown-information).

+## Manage a shared GitHub connection
+
+[D · g5.settings.github-connection]
+
+When the deployment offers the GitHub App, offer disconnected users optional GitHub
+authorization after login, preserving their intended destination and allowing them to
+continue without connecting. Account settings owns connected-account status, connection,
+reconnection and disconnection. PR and issue-comment surfaces direct users to this
+shared account connection; neither creates its own grant. Connection errors do not
+prevent platform sign-in, and a stored grant is not proof of current repository access.
+
+Governing: [platform login remains separate](../concepts/security.g5.md#keep-platform-login-distinct-from-repository-integration);
+[PR authorship](governance.g5.md#attribute-github-pr-creation-to-the-initiating-user)
+and external issue authorization retain their operation-specific checks.
+
 ## Metadata

 ### g5.story.set-spending-limit
@@ -132,3 +147,9 @@ Accounting follows the [usage evidence rules](agents.g5.md#usage-reports-preserv
 - agent
 - Retained [cost visibility §§1/5](../../../docs/reference/designs/DESIGN.md), [spend history §3.15](../../../docs/reference/designs/API_AND_STORAGE.md) and [v9 §8.8](../../../docs/lean-backend-design-v9.md).
 - ~55% · `spend_service.rs`, settings routes and `SpendBreakdown.tsx`; history reads usage records, but current summaries join jobs and count only running/succeeded tasks, omitting jobless chats and consumed work in other outcomes; billing reconciliation is unexecuted.
+
+### g5.settings.github-connection
+
+- human-verified
+- [Central connection and optional login offer](../../chat_intent/codex-pr-user-authorship/request.md).
+- Implemented in `GithubConnectionCard`, `GithubConnectionPage`, `UserSettingsPanel`, `LoginPage` and `useGithubConnection`, reusing existing user-grant endpoints. Frontend tests verify optional login connection, nested return paths, failures, disconnect and PR/comment recovery. Live browser OAuth remains unexecuted. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md).
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 90c8b797..f087d997 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -17,6 +17,8 @@ and [governance](../features/governance.g5.md); post-MVP governance retains its
 |---|---|---|
 | Git policy / storage adapters | Checked `/git/:repo_id` receive-pack, protected-ref import, publication and merge | [Ref effects](#git-policy-owns-authoritative-ref-effects) |
 | CoordinatedRepoPublicationService / PreparedBranchCommit / ApprovalReviewService | Prepare/apply; `land*`; approval `admit`, `settle_terminal` | [Governance coordination](#publication-and-approval-coordination) |
+| CoordinatedRepoPublicationService / WorkspacePullRequestProviderLifecycleService | `list_workspace_pull_requests`, `read_github_pull_request`; public and incarnation-authenticated `GET /workspaces/:id/pull-requests/info` | [PR retrieval](../features/governance.g5.md#retrieve-pull-requests-through-the-cli) |
+| Shared create/update operation | UI and CLI create/update; public and current-incarnation adapters | [One PR flow](#one-create-or-update-pr-flow) |
 | DocService | `list_docs`, `get_doc`, `live_doc_read`, `sync_view_by_ref`, `create_explainer` | [Documents](#document-operations) |
 | OntologyService | `get_ontology_tree`, `get_node`, `get_node_dependencies`, `resolve_linkable_node_ids`, `sync_specs_for_branch` | [Semantic interpretation](#ontology-interpretation) |
 | CommentService | Anchored comment CRUD and replies | [Comments](#comment-anchors-and-replies) |
@@ -38,6 +40,27 @@ operations over reusable `RepoStorage`, `GitRepo` and workspace Git mechanisms.
 Git/governance coordinates publication and merge over exact Git versions through
 prepare/apply/land operations and separately authorized approval decisions.

+### One create-or-update PR flow
+
+[D · g5.architecture.pr_flow]
+
+One Git/governance operation owns target choices/default resolution, title input, source/target
+admission, saving and pushing source work, PR evidence capture and reuse, source publication,
+and provider PR creation/update. The
+UI, signed-in CLI and incarnation-authenticated VM CLI enter that operation through thin
+adapters; they cannot assemble or duplicate its sequence. Authentication remains typed,
+and current VM authority is checked at the boundaries that admit effects. Standard creation
+retains the review-workspace restriction; review-specific handback remains a separate capability.
+Automatic [push continuation](../features/governance.g5.md#follow-successful-source-pushes-automatically)
+reuses this owner's committed-work publication stages for an existing PR, without invoking
+explicit save/push preparation. Git entry points signal accepted ref changes; this owner
+handles durable discovery/retry, current authorization and PR/provider evidence.
+
+Governing: [agent PR creation](../features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent),
+[publication readiness](../features/governance.g5.md#refuse-pr-creation-while-work-is-not-ready),
+[owner authority](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) and
+[current token authority](../concepts/security.g5.md#admit-actions-against-current-authority).
+
 ### Document operations

 [D · g5.architecture.semantic_surfaces]
@@ -80,7 +103,13 @@ synchronization to API maintenance through shared typed Git readers.

 - agent
 - [Design v9 §10](../../../docs/lean-backend-design-v9.md#10-governance).
-- ~50% of target governance · `backend/g5-api-core/src/services/{coordinated_repo_publication, branch_commit, approval_review_service}.rs`; partly legacy implementation, eligible nonowner human approval is excluded, redesigned merge recovery is unassessed, and mutation-policy retirement awaits its P6 replacement.
+- ~50% of target governance · `backend/g5-api-core/src/services/{coordinated_repo_publication, branch_commit, approval_review_service}.rs`; partly legacy implementation, eligible nonowner human approval is excluded, redesigned merge recovery is unassessed, and mutation-policy retirement awaits its P6 replacement. The read-only CLI/API addition uses the existing durable list and `workspace_pull_request_read.rs`; scoped user and current-incarnation routes pass integration tests with mocked GitHub. Shared UI/CLI writes are coordinated by `workspace_pull_request_create.rs` with typed admission and explicit partial-provider recovery; its scoped assessment is in `g5.architecture.pr_flow`. [CLI review](../../chat_intent/codex-pr-user-authorship/review.md#cli-retrieval-consolidated-discovery-before-repair).
+
+### g5.architecture.pr_flow
+
+- human-provided
+- [One create/update flow requirement](../../chat_intent/codex-pr-user-authorship/request.md); the user explicitly requires exactly the same flow as the UI buttons.
+- Automatic push continuation shares the evidence/provider owners with explicit creation, using published refs and current ownership without VM preparation. Durable pending state, bounded provider reads, scope-reusing credential lookup and PR serialization support recovery; native/API/concurrency/migration and all three PR UI consumers are covered in the [automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Target choices, omitted-target resolution and title input share this owner; the provider lifecycle consumes durable pending title intent under a PR row lock. API/CLI/UI follow-up coverage includes failure/retry, concurrent title acceptance and target-cache refresh. [Follow-up evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Implemented in `backend/g5-api-core/src/services/workspace_pull_request_create.rs`; public and internal route adapters share the full sequence and current authority checks. Real HTTP tests, scripted native peers, mocked providers and compiled CLI/UI consumers cover reuse, refusals and partial-error recovery. Standard creation retains review restrictions. Actual active guest/native callback and live-provider execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment).

 ### g5.architecture.semantic_surfaces
```

The comparison also adds the PR retrieval API row and updates `g5.architecture.governance_owner` assessment in [Git/governance architecture](../../ontology/software_architecture/git_governance.g5.md#public-operations).

Add `g5.settings.github-connection` and its metadata to [settings](../../ontology/features/settings_and_usage.g5.md#manage-a-shared-github-connection).

Add `g5.architecture.pr_flow` in [Git/governance architecture](../../ontology/software_architecture/git_governance.g5.md#one-create-or-update-pr-flow); the JSON contract now includes create/update results and pre-mutation validation.

**Follow-up changes:** `g5.governance.pr.write`, `g5.governance.pr.json` and
`g5.architecture.pr_flow` are amended; `g5.governance.pr.target` and
`g5.governance.pr.title` are added in the diff above.

**Automatic push follow-up:** add `g5.governance.pr.follow-push` in governance and
amend `g5.architecture.pr_flow` in Git/governance architecture (diff above).

| Commitment | Obligation / owner | Allowed case / counterexample | Conclusion |
|---|---|---|---|
| [g5.governance.pr.follow-push](../../ontology/features/governance.g5.md#follow-successful-source-pushes-automatically) | Existing PRs follow accepted committed pushes through shared publication | Push B after PR A / rejected push or obsolete event replacing B with A | Durable discovery/retry; no new G5 PR, retarget, autosave or compute |
| [g5.governance.approval.version / merge](../../ontology/features/governance.g5.md#bind-human-approval-to-the-version-reviewed) | Refresh published evidence without renewing approval or unresolved merge intent | B appears in PR, approval A is stale / approval silently advances or concurrent merge repins | Preserve exact version checks and merge identity |
| [g5.security.admission / tenant / read](../../ontology/concepts/security.g5.md#admit-actions-against-current-authority) | Continuation uses current eligible owner, same organization and readable target | Authorized existing PR / changed owner or lost target access using stale authority | Recheck at effects; provider credentials remain server-side |
| [g5.architecture.pr_flow](../../ontology/software_architecture/git_governance.g5.md#one-create-or-update-pr-flow) | One owner for manual and automatic committed publication | Native push, UI push or CLI reaches same stages / callback copies provider SQL or recursively autosaves | Inspect reused operations before reuse; factor authorized existing-PR continuation |
| [g5.architecture.recovery / recovery.progress](../../ontology/concepts/system_design_principles.g5.md#make-failures-and-retries-explicit) | Successful push is independent of recoverable PR refresh | Provider unavailable, restart, multi-repo partial push / lost obligation or starvation | Durable work discovery, visible failure, current-head coalescing |

**Relevant and governing ontology:**

| Commitment | Obligation and owner | Allowed case / counterexample | Conclusion |
|---|---|---|---|
| [g5.governance.pr.target](../../ontology/features/governance.g5.md#infer-the-pr-destination-consistently) | Shared owner supplies ordered choices and resolves/pins omitted target | Readable parent or Base / inaccessible default or later silent retarget | UI consumes server choices; resolve before effects, preserve later access checks |
| [g5.governance.pr.title](../../ontology/features/governance.g5.md#accept-an-explicit-pr-title) | Shared title input reaches G5/provider records | Explicit title on create/reuse / omission overwrites existing title | Preserve existing titles on omission; use normal provider failure reporting |
| [g5.governance.pr.write / g5.architecture.pr_flow](../../ontology/features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent) | One create/update owner serves UI and CLI; adapters only translate/authenticate | Same source/target via UI or VM follows identical pipeline / duplicated prepare or provider path | Extract/reuse one orchestration; `update` aliases create-or-update |
| [g5.security.owner / admission / login.service-identities](../../ontology/concepts/security.g5.md#one-eligible-owner-uses-live-compute), [g5.runtime.lease](../../ontology/software_architecture/runtime.g5.md#short-server-lease-and-long-credential-have-separate-purposes) | Current VM identity delegates only its source owner’s PR action; recheck at effect boundaries | Current owner/VM/epoch/lease with readable target / stale, stopped, transferred, wrong-source or human-token confusion | Typed narrow adapter, no fabricated human login; inspect authority helpers during coding |
| [g5.governance.pr.json](../../ontology/features/governance.g5.md#select-pr-json-fields-using-familiar-syntax) | Create/update select G5 fields as objects; help/invalid fields cannot mutate | Field help without auth / bare JSON accidentally publishes | Validate before POST; preserve current read behavior |

| [g5.governance.publish.github-author](../../ontology/features/governance.g5.md#attribute-github-pr-creation-to-the-initiating-user) | Credential owner selects initiating user's App grant for new PRs, enforcing binding coverage and permissions | Authorized connected user / no bot or teammate fallback on missing grant | New explicit creation operation; preserve existing PR author |
| [g5.governance.publish.actor](../../ontology/features/governance.g5.md#publish-under-the-acting-humans-provider-authority) | Provider enforces user access; credentials resolved server-side | User has access / installation access alone cannot authorize user PR creation | Reuse user grant owner; inspect its refresh/failure policy in coding |
| [g5.governance.publish, g5.governance.publish.readiness](../../ontology/features/governance.g5.md#publish-existing-work-under-the-actors-provider-authority) | Publication owner operates on ready committed branch state | Ready source / failed preparation or dirty stopped snapshot | Shared save/push precedes evidence; later P6 remains separate |
| [g5.security.admission, g5.security.tenant, g5.security.tenant.write](../../ontology/concepts/security.g5.md#admit-actions-against-current-authority) | Routes authorize action; resolver checks actor membership and repository identity | Current actor and covered repo / cross-tenant or uncovered repo | Preserve all checks before using grant |
| [g5.security.credentials.publication](../../ontology/concepts/security.g5.md#keep-provider-publication-credentials-server-side) | Credential owner keeps tokens out of guest and public output | Server GitHub request / leaked token in error | Reuse encrypted grant and refresh mechanism |
| [g5.architecture.governance_owner](../../ontology/software_architecture/git_governance.g5.md#publication-and-approval-coordination) | PR service coordinates provider operations without owning token storage | Request creation credentials / caller assembling grant SQL | Extend existing credential API |
| [g5.design.minimal-mechanism, g5.design.change-locality](../../ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | Reuse existing App grant infrastructure and connection UI | Scoped PR change / rewrite push and merge credentials | Reuse existing owners; one nullable pending-title column supports durable retry, with no unrelated policy change |
| [g5.quality.review.cases, g5.quality.evidence.claims](../../ontology/concepts/coding_principles.g5.md#review-both-permitted-and-forbidden-behavior) | Test owner verifies authority boundary and reports exact evidence | Connected actor, missing/revoked grant, binding refusal, non-App compatibility / happy-path-only test | Boundary tests plus source review |

| [g5.governance.pr.read](../../ontology/features/governance.g5.md#retrieve-pull-requests-through-the-cli) | CLI/API expose read-only workspace list/view/status and scoped GitHub lookup | Current workspace or attached repository / arbitrary URL or unrelated repository | Add narrow read operation; verify existing readers have no reconciliation effects |
| [g5.security.read, g5.security.read.durable, g5.security.admission](../../ontology/concepts/security.g5.md#read-permission-is-explicit) | Read route enforces workspace visibility and current typed VM authority, uses durable records | Eligible reader or current VM token / stale incarnation or private workspace outsider | Reuse checked read entry point after source inspection |

| [g5.governance.pr.json](../../ontology/features/governance.g5.md#select-pr-json-fields-using-familiar-syntax) | CLI parser and formatter select supported fields without wrappers, disclose G5 status semantics and preserve other commands | View object/list array and field discovery / silent unknown fields or changed non-PR flag parsing | PR-scoped JSON option with root boolean kept for other commands; validate before I/O |

### Committed review assessment delta

The original branch-wide design diff above is retained. The commit review makes no new
product-policy commitment. It updates the footer assessments for `g5.governance.pr.write`,
`g5.governance.pr.follow-push`, `g5.governance.pr.target`, `g5.governance.pr.title`,
`g5.architecture.pr_flow`, `g5.design.authority` and `g5.architecture.recovery.progress`.
The last two already govern current-state consumers and fair retry; their remaining system
coverage stays partial. Exact assessment diff relative to reviewed commit `f2b2d9f5`:

```diff
diff --git a/.g5/ontology/concepts/system_design_principles.g5.md b/.g5/ontology/concepts/system_design_principles.g5.md
index 83a7ffa7..de6c5933 100644
--- a/.g5/ontology/concepts/system_design_principles.g5.md
+++ b/.g5/ontology/concepts/system_design_principles.g5.md
@@ -254 +254 @@ Changes to a persistent or public contract must preserve existing valid state th
-- Partial · Git/PG/projection services; OID matching across every UI and snapshot consumer remains unverified. Cross-owner progress projections are unassessed in this checkout.
+- Partial · Git/PG/projection services; OID matching across every UI and snapshot consumer remains unverified. Cross-owner progress projections are unassessed in this checkout. PR source dialogs follow cache-backed records after create/update, with stale-read cancellation and mounted failure/recovery evidence in the [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
@@ -266 +266 @@ Changes to a persistent or public contract must preserve existing valid state th
-- Unassessed · maintained recovery workers have not been checked for starvation or persisted scheduling progress; the experimental candidate does not establish a defect in this checkout.
+- Partial · the PR push-refresh worker uses bounded SQL lock waits and a per-item deadline; a real held-lock regression verifies later-organization progress and retry after unlock. Other maintained recovery workers remain unassessed; the experimental candidate does not establish their state. [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 4c89e0d3..c187076f 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -367 +367 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented by `workspace_pull_request_create.rs`, thin public/internal routes, runtime fencing and `g5d-cli/src/pr.rs`. Public/VM HTTP tests verify reuse, current source/target authority, save/push ordering and explicit partial provider errors; compiled CLI and UI tests verify projection, recovery and refreshed PR lists. Actual active-agent/native Git callbacks, live GitHub and deployed workers remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment).
+- Implemented by `workspace_pull_request_create.rs`, thin public/internal routes, runtime fencing and `g5d-cli/src/pr.rs`. Public/VM HTTP tests verify reuse, current source/target authority, save/push ordering and explicit partial provider errors; compiled CLI and UI tests verify projection, recovery and refreshed PR lists. Actual active-agent/native Git callbacks, live GitHub and deployed workers remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment). Queued UI/VM writes and provider admission retain current target access across PR waits; refused metadata and authorized retries are checked in the [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
@@ -373 +373 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 056 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment).
+- Implemented in `workspace_pull_request_create.rs`, shared evidence/provider owners, `maintenance/workspace_pull_requests.rs`, migration 056 and PR UI/event consumers. Native receive-pack, published multi-member cuts, approval-version retention, current target access, provider retry/timeout, omitted-event recovery, concurrent UI refresh, App credentials and accepted-provider race regressions pass. Populated migration upgrade/replay passes under a non-BYPASSRLS owner. Evidence uses real local Git, restricted tenant roles and mocked GitHub; live GitHub, deployed workers and a process restart remain unverified. [Automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Contended SQL admission/error recording is bounded, and later organizations progress before the lock is released. Open source dialogs consume current query state after mutations; see [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
@@ -379 +379 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented by `workspace_pull_request_create.rs` and `useWorkspacePullRequestTargets`; API tests cover public/VM defaults, ordered readable parent/Base choices, invalid destinations and access loss without retargeting. A mounted hook test verifies choices refresh after workspace deletion. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment).
+- Implemented by `workspace_pull_request_create.rs` and `useWorkspacePullRequestTargets`; API tests cover public/VM defaults, ordered readable parent/Base choices, invalid destinations and access loss without retargeting. A mounted hook test verifies choices refresh after workspace deletion. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Revocation during queued UI/VM local and provider phases is covered in the [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
@@ -385 +385 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Implemented in shared creation, coordinated publication and provider lifecycle services, with migration 055 retaining pending title intent. Public API tests with mocked GitHub cover create/reuse/omission, failure and retry, preserved provider identity, overlapping title requests serialized on the PR row, and existing unchanged-member PRs. Compiled CLI tests cover title transport and validation. Live GitHub execution remains unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment).
+- Implemented in shared creation, coordinated publication and provider lifecycle services, with migration 055 retaining pending title intent. Public API tests with mocked GitHub cover create/reuse/omission, failure and retry, preserved provider identity, overlapping title requests serialized on the PR row, and existing unchanged-member PRs. Compiled CLI tests cover title transport and validation. Live GitHub execution remains unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). A concurrent close refuses title acceptance and preserves terminal metadata; [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment).
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index f087d997..0ac68717 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -112 +112 @@ synchronization to API maintenance through shared typed Git readers.
-- Automatic push continuation shares the evidence/provider owners with explicit creation, using published refs and current ownership without VM preparation. Durable pending state, bounded provider reads, scope-reusing credential lookup and PR serialization support recovery; native/API/concurrency/migration and all three PR UI consumers are covered in the [automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Target choices, omitted-target resolution and title input share this owner; the provider lifecycle consumes durable pending title intent under a PR row lock. API/CLI/UI follow-up coverage includes failure/retry, concurrent title acceptance and target-cache refresh. [Follow-up evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Implemented in `backend/g5-api-core/src/services/workspace_pull_request_create.rs`; public and internal route adapters share the full sequence and current authority checks. Real HTTP tests, scripted native peers, mocked providers and compiled CLI/UI consumers cover reuse, refusals and partial-error recovery. Standard creation retains review restrictions. Actual active guest/native callback and live-provider execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment).
+- Automatic push continuation shares the evidence/provider owners with explicit creation, using published refs and current ownership without VM preparation. Durable pending state, bounded provider reads, scope-reusing credential lookup and PR serialization support recovery; native/API/concurrency/migration and all three PR UI consumers are covered in the [automatic refresh assessment](../../chat_intent/codex-pr-user-authorship/review.md#automatic-refresh-final-assessment). Target choices, omitted-target resolution and title input share this owner; the provider lifecycle consumes durable pending title intent under a PR row lock. API/CLI/UI follow-up coverage includes failure/retry, concurrent title acceptance and target-cache refresh. [Follow-up evidence](../../chat_intent/codex-pr-user-authorship/review.md#target-title-and-node-id-final-assessment). Implemented in `backend/g5-api-core/src/services/workspace_pull_request_create.rs`; public and internal route adapters share the full sequence and current authority checks. Real HTTP tests, scripted native peers, mocked providers and compiled CLI/UI consumers cover reuse, refusals and partial-error recovery. Standard creation retains review restrictions. Actual active guest/native callback and live-provider execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#shared-pr-writes-repairs-and-final-assessment). The [commit review](../../chat_intent/codex-pr-user-authorship/review.md#committed-change-review-f2b2d9f5--final-assessment) verifies queued target authority, terminal title protection, contended-worker progress and post-mutation UI freshness.
```

### Workspace CLI usability amendment

Compared with `4dc163da`, add `g5.runtime.workspace-cli` and
`g5.runtime.workspace-cli.context` in [runtime](../../ontology/software_architecture/runtime.g5.md#make-the-workspace-cli-available-to-people-and-agents).
The user's “close the gaps” and direct-call follow-up authorize implementation.
The reserved-environment footer also receives a scoped evidence correction; its governing
intent is unchanged.

```diff
diff --git a/.g5/ontology/concepts/security.g5.md b/.g5/ontology/concepts/security.g5.md
index 886c5f14..e3843f19 100644
--- a/.g5/ontology/concepts/security.g5.md
+++ b/.g5/ontology/concepts/security.g5.md
@@ -234,7 +234,7 @@ GitHub, GitLab and local-only repositories remain supported integration concepts

 - human-verified
 - Human scope: [H083–H087](../../uplift/g5-lean/candidates/human-decisions.md), [M008–M009](../../uplift/g5-lean/candidates/scope-resolutions.md); agent key/redaction safeguards: [C090](../../uplift/g5-lean/candidates/contracts.md).
-- ~50% · dispatch/runner validate syntax and overwrite some identity fields; HOME, PATH, provider homes and Git base environment can still be shadowed.
+- Partial · `is_exportable_task_env_key` rejects G5/G5D prefixes, HOME and PATH at terminal and runner admission; runtime assembly supplies CLI identity. A real terminal rejects a workspace-token override in the [CLI review](../../chat_intent/codex-pr-user-authorship/review.md#workspace-cli-usability--final-assessment). Broader provider-home and Git-base environment shadowing remains outside this scoped assessment.

 ### g5.security.credentials.redaction

diff --git a/.g5/ontology/software_architecture/runtime.g5.md b/.g5/ontology/software_architecture/runtime.g5.md
index 7c45c067..3adc968b 100644
--- a/.g5/ontology/software_architecture/runtime.g5.md
+++ b/.g5/ontology/software_architecture/runtime.g5.md
@@ -110,6 +110,30 @@ Use image pins only from committed restored code-member HEADs, falling back to t

 Supply a complete read-only runtime, provider CLIs, libraries and pinned module catalog independently of the customer image. Reject missing/incompatible components before readiness; no guest-copy fallback. [Module authority](workspace.g5.md#extend-through-declared-modules) governs extensions.

+## Make the workspace CLI available to people and agents
+
+[D · g5.runtime.workspace-cli]
+
+Workspace terminals and provided-agent shells can invoke `g5d` by name through one
+runtime-owned launcher setup, preserving image tools and applying G5 library paths only
+to G5 processes. The guest runtime supplies `g5d` in the standard command path as well
+as the inherited PATH so ordinary login shells can find it.
+
+## Supply the current workspace context to the CLI
+
+[D · g5.runtime.workspace-cli.context]
+
+Owner terminals and provided agents receive the same API URL, current workspace ID and
+incarnation token for direct `g5d` access, without another login or a separate local PR
+proxy. This follows the accepted [guest token boundary](../concepts/security.g5.md#expose-the-incarnation-token-under-the-mvp-boundary),
+not a grant of host/dispatcher control credentials. Existing [PR operations](../features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent),
+[current authority](../concepts/security.g5.md#admit-actions-against-current-authority)
+and [publication credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side)
+remain server-owned. User secrets cannot replace this context. CLI context does not
+broaden the selected [agent permission policy](../features/agents.g5.md#freeze-provider-permissions-before-spawning):
+network-disabled agents cannot call the API and must report the block for owner-terminal
+execution rather than bypass the sandbox.
+
 ## Host Git never trusts retained guest control files

 [D · g5.runtime.materialization]
@@ -258,6 +282,18 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
 - From [C085/C094](../../uplift/g5-lean/candidates/contracts.md), [S021](../../uplift/g5-lean/candidates/supporting-docs.md); [R03](../../uplift/g5-lean/investigations/runtime-security.md#r03--runtime-bundle-readiness-has-a-narrower-check-than-its-documented-contract).
 - ~65% · runtime bundle/catalog validation; readiness does not establish complete executable, provider CLI, library and pin closure.

+### g5.runtime.workspace-cli
+
+- human-verified
+- [Workspace CLI usability request](../../chat_intent/codex-pr-user-authorship/request.md); shared launcher and standard guest command path are implementation decisions.
+- Shared `runtime_environment::install_g5d_launcher` serves dispatcher and provider shells. Quoting, image PATH, scoped libraries and real terminal CLI checks pass; Linux root-to-UID-1000 execution repairs a retained 0700 directory. Full Linux guest standard-path/login-shell installation remains unexecuted. [Review](../../chat_intent/codex-pr-user-authorship/review.md#workspace-cli-usability--final-assessment).
+
+### g5.runtime.workspace-cli.context
+
+- human-verified
+- [Direct CLI follow-up](../../chat_intent/codex-pr-user-authorship/request.md) applies the existing guest token model to owner terminals.
+- Shared environment builder supplies dispatcher terminals and agent.turn. A real PTY/CLI with mock API verifies inferred workspace/auth, read/create, actionable write failure and reserved-token rejection. Existing server authority and publication flow are unchanged. Restricted Codex shell networking remains intentionally unavailable under the governing policy; real model choice, live GitHub and deployed guest checks remain unexecuted. [Review](../../chat_intent/codex-pr-user-authorship/review.md#workspace-cli-usability--final-assessment).
+
 ### g5.runtime.materialization

 - agent
```

**Governing commitments:** `g5.runtime.workspace-cli` owns shared launcher availability;
`.context` and `g5.security.credentials.platform-token` supply the same accepted
workspace-scoped API context to agents and owner shells. `g5.terminal` retains the native
owner shell; `g5.security.owner`, `.admission` and `g5.runtime.lease` require current
owner/incarnation checks at the existing API. `g5.security.credentials.publication` keeps
GitHub tokens server-side; `.reserved-env` prevents settings from replacing CLI identity.
`g5.governance.pr.read`, `.write` and `.json` preserve the single UI/CLI flow, selection and
output contracts. `g5.design.minimal-mechanism`, `.reuse` and `.adapters` favor direct
existing CLI/API transport. `g5.quality.review.cases` and `.evidence` govern boundary tests
and distinguish fixture results from actual agent choice/live GitHub.
`g5.agents.admission.permission-policy` preserves the frozen permission/sandbox choice:
shared context does not authorize network access in network-disabled modes. Initial
review exposed an overbroad acceptance claim; the original handoff and finding remain in
the review artifact. Owner-terminal execution is the explicit recovery for those modes;
providing narrow agent API connectivity remains outside this implementation.

| Commitment → owner | Allowed case / counterexample | Design conclusion |
|---|---|---|
| Workspace CLI → shared launcher | Terminal and provider shell resolve `g5d` / G5 libraries override ordinary image tools | Expose only the G5 launcher; retain image PATH order for other commands. |
| Workspace context → runtime setup | Current terminal uses the same identity as agents / settings replace workspace or token | Share the three context bindings and preserve reserved-key admission. |
| Current authority and publication → existing API | Terminal creates a PR through the existing operation / stale or foreign source writes | Direct CLI adapter and current server checks remain unchanged. |
| Recovery → existing PR operation | Partial publication or lost reply / automatic resubmission of a write | Preserve existing one-attempt create and actionable recovery. |

## Outcome and design

The user's request authorizes implementation on `codex/pr-user-authorship`. New GitHub
PR creation uses the initiating actor's GitHub App user token when an App binding exists.
Use a distinct creation operation so Git reads, pushes, PR reconciliation and merges keep
their existing policy. The credential owner must retain repository URL, organization,
binding coverage and permission checks. The existing user grant owner retains encryption,
refresh and revocation. The PR service surfaces a connection/reconnection action when
that grant is unavailable and does not silently use installation or personal SCM fallback.
Without an App binding, preserve personal SCM behavior. Existing PR discovery/reuse
retains its current author. Do not introduce a second user connection or token store.

## Acceptance outcomes

- A new App-bound PR uses only the initiating actor's connected GitHub token.
- Missing/revoked authorization returns an actionable failure without a bot-authored PR.
- Inactive, uncovered and insufficient-permission bindings still fail closed.
- Installation authentication remains for push and other operations; non-App PRs still
  use the acting user's personal SCM connection.
- Existing PR reconciliation cannot change authorship or duplicate the PR on retry.

## Design self-review and implementation assumptions

One deliberate alternate retrieval pass followed the invariant index into admission,
tenant/write scope and server-only publication credentials, then the Git/governance
module's minimal API and policy-variant references. These require a distinct creation
operation rather than globally changing App token preference. External issue publication
also forbids bot substitution, but its separate destination/content policy does not govern
PR creation. No material intent question remains. Source inspection verified grant refresh ownership, PR entry paths, binding selection
and permission checks, and the shared user-grant endpoints before dependent implementation. Source review and boundary
tests will be distinguished from live GitHub verification.

## Central connection amendment

The user selected an optional connection offer after login and management in Account
settings. Keep existing identity OAuth. Reuse the same user-grant status, start and
disconnect endpoints from a shared hook and account connection card. A central authenticated
connection page preserves a validated local return destination through OAuth. Login offers
it when App support exists and the user is disconnected; connected users proceed, and
skip/error always allows continuing. PRs and comments link to that page. The Account
panel shows the connected GitHub login, reconnect and disconnect actions. Queries are
keyed to the signed-in G5 user. No App grant or permission is inferred from platform login.

Governing `g5.settings.github-connection` requires one shared grant, optional login offer,
non-blocking connection failures and safe return paths. `g5.security.login` still separates
identity from repository access. Acceptance covers connected/disconnected login, skip,
failed connection, account reconnect/disconnect, disabled deployments, safe return paths,
and both PR and comment recovery navigation. Self-review confirms no new login provider,
App installation, permission expansion or token storage is introduced. Original handoff
is retained in review.md. The repository credential owner additionally probes repository
access with the user token to detect revocation before PR creation; GitHub still decides
write permission at POST. This reuses the existing App client and keeps tokens server-side.


## Final review decisions

PR request destinations derive from the repository's validated Git URL. Before issuing
creation credentials, the App client probes that destination using the user token and
requires its stable GitHub ID to match the selected binding. A separate imported display
name cannot redirect a user's broader grant; reassigned names are refused until the
binding is refreshed. This implements the existing repository-restriction commitment.

After sign-in, an explicitly requested account connection page resumes directly, retaining
its nested return destination. Other login destinations receive the optional offer. The
same card and grant endpoints serve Account settings, post-login setup and feature recovery.
No material design assumptions remain open. Review repairs and their runtime evidence are
recorded in [review.md](review.md); live GitHub/OAuth remains unverified.

## CLI retrieval amendment

The user requests implementation of a `gh pr`-shaped read interface and selected both
workspace PRs and GitHub lookup. Add `g5d pr list`, `g5d pr view [number-or-URL]` and
`g5d pr status`, with workspace override and JSON output following CLI conventions.
Without a provider selector, use stored G5 PR records for the current workspace. With
a number or canonical GitHub PR URL, retrieve live title/body/state/author/refs and
other provider detail exposed by the read adapter. Support explicit repository selection
when multiple workspace repositories make a number ambiguous. Keep this initial surface
read-only; provider mutations, generic arbitrary-repository browsing and full `gh` flag
parity are outside this request.

The CLI owns argument parsing and presentation; the API authorizes workspace reads and
resolves attached repository identity, while the existing credential owner applies the
operation-specific read policy. GitHub tokens remain server-side. Provider URLs are
identifiers to validate, never arbitrary request destinations. Use bounded provider
requests and honest errors; stored workspace PR state is not labeled as live GitHub state.
No new token store, background sync, compute admission or schema is needed.

Acceptance: current-workspace defaults; explicit workspace selection; readable and JSON
output; empty list and missing PR behavior; GitHub number and URL selection; ambiguous,
unattached and foreign-host rejection; workspace reader/tenant checks; stale VM rejection;
provider failures without leaked credentials or mutation. Reused read operations, repository
credentials and token checks must be inspected before use. Alternate retrieval through
system invariants and security confirms reads cannot supply compute consent and durable
reader authority differs from owner-only live mutations. Self-review found no unresolved
intent question; implementation may refine flags and field representation in this document.

CLI implementation details: `--workspace-id UUID` selects workspace context, while the
existing global `--workspace PATH` selects a local checkout. The JSON follow-up below
replaces the initial PR boolean JSON interface with field selection; other commands retain
the existing boolean flag. `list` defaults to open,
30 records and accepts `--state`/`--limit`; `status` summarizes open incoming/outgoing
records; `view` accepts G5 UUIDs in addition to explicit GitHub selectors and defaults to
the newest open outgoing G5 PR, falling back to the newest outgoing PR. `--comments`
reads the first 100 GitHub discussion comments and reports whether more exist. Formal
reviews, inline review comments, checks and diffs remain outside this initial surface.
The saved read design permits server-side credential refresh/minting but no PR/domain
mutation. Source inspection confirmed the existing G5 list has no reconciliation effects;
provider refresh does, so live lookup has its own bounded read operation. Both authenticated
routes use the same reader with workspace or current-incarnation admission.

Post-review decisions: public and VM adapters live beside their existing audited route
registrations and call one shared read adapter. The exact internal GET route is registered
with incarnation authentication before the handler revalidates workspace identity and
current owner/VM/epoch/lease. Explicit `--workspace PATH` selects that checkout's identity
and never silently falls back to ambient workspace identity; an incompatible VM credential
fails. These are repairs under the existing design, not authorization-policy amendments.

## JSON compatibility amendment

The user asks to update JSON compatibility. `g5d pr view 42 --json title,url` returns a
plain selected object; `list` returns an array of selected objects. Workspace `status`
retains its workspace semantics and therefore returns an array rather than inventing
GitHub's authored/review-request categories. `--json=title,url` is equivalent. Bare
`--json` lists supported fields before workspace discovery/auth/network; unknown or empty
field names produce actionable errors, including explicit `--json=` and `--json ""`.
Only omitting the optional value requests field discovery; the root boolean
`g5d --json pr ...` also discovers fields. Preserve other commands' boolean flag and argument
placement. PR field selection is local presentation, not permission or retrieval scope.

Support the fields the current reader actually supplies. Normalize GitHub mergeability
values for selected output; G5 records expose native fields plus common PR aliases only
when their meaning is unambiguous. Single-provider fields are null for multiple providers;
never synthesize checks/review/author facts. Keep source selectable as a G5 extension.
Requesting comments on explicit GitHub views must fetch them even without `--comments`,
and retain the existing bounded page/has-more information. jq/template formatting remains
outside this focused amendment, and command/workspace selection semantics are unchanged.

Reused parser/global-flag behavior must be inspected and exercised across PR and non-PR
commands; keep the change localized without changing every consumer of GlobalFlags.
Acceptance covers option placement, equals syntax, unknown/empty fields, no-context field
discovery, view/list shapes, null/multi-provider semantics, comment demand and legacy command
parsing. Alternate retrieval through `g5.governance.pr.read`, security read/token boundaries
and minimal-mechanism principles confirms this can remain a CLI adaptation without a new
provider API or credential policy. The existing server JSON retains its internal shape.


## Shared create/update CLI amendment

Implement the user's requested missing write adapters. `g5d pr create` and `g5d pr update`
are names for the same UI-compatible upsert, not separate publication algorithms. Use the
current workspace as source and retain explicit `--workspace-id`/checkout selection. The
target/title amendment below supersedes the initial required-target contract. Support an
optional description through `--body` or `--body-file` with conflicts checked before effects. JSON uses the
existing G5 field selection and a selected object; bare/invalid fields perform no mutation.

The shared owner performs the entire ordered UI operation, including source restrictions,
target access, save/push preparation, evidence/reuse, publication and provider synchronization.
Do not leave UI-only guards outside a boundary the CLI can bypass. Standard creation retains the review-workspace restriction; existing review-specific
handback stays a separate capability. Authentication
adapters preserve human and VM identities; a VM can act only for its current source owner,
with live owner/VM/epoch/lease and stop/transfer checks at effect-admission boundaries. It
cannot manufacture general user authority, select another source, or receive GitHub tokens.
No schema, new credential store, new PR algorithm, merge or approval command is needed.

Reused operations are assumptions until source inspection establishes their exact checks,
transaction lifetimes, asynchronous effects and retry behavior. In particular, verify the
existing UI save/push operation can be invoked from a running agent without holding a lock
needed by its callback. Preserve owner restrictions and provider authorship; do not silently
weaken admission to reuse a helper. Both surfaces must return the same PR/provider outcome,
including partial failure, with CLI errors and human output making recovery visible.

Acceptance: real public and VM requests enter one orchestration; a valid VM can create then
reuse/update the same PR; dirty source saving and push/publication order match the UI; denied
source/target and stale/replaced/expired/transferred VM attempts cause no new effects; typed
human/VM/host credentials cannot cross adapters. Exercise provider failures, blocked source
states, review-workspace restrictions and safe retry, plus compiled CLI transport, body handling,
JSON non-mutation help/validation and read regressions. Add worker skill documentation for
both bundled agent profiles with source/target, auto-save effects and provider recovery.

Design self-review: retrieved the security service-identity exception and runtime lease via
the system-invariant index in addition to publication rules. Long-lived credentials do not
confer current authority; CLI entry does not imply permission to start compute. Shared policy
must preserve the authorized review-handback variant. The smallest design is one existing
operation with typed admission and thin adapters; no unresolved product-intent question blocks
implementation. The user has explicitly requested implementation of this same-flow design.


### Source-inspection clarification for shared writes

The old route comment named review Finish as a caller, but the current call graph has only
the public Create/Update adapter; review workspaces publish suggestions through their own
capability. Standard create/update must retain `ensure_not_review_workspace`, and the CLI
adds no handback exception. The architecture wording now states that precise boundary.
The retained readiness paragraph now distinguishes explicit UI/CLI save/push preparation
from later P6's caller-checkpoint/not-ready policy. The user requested the UI flow after its
auto-save behavior was explained; this preserves the requested current behavior and the
separate later P6 scope rather than silently choosing a conflicting readiness interpretation.

The shared operation is extracted from the UI route. It uses typed user or VM callers,
scoped actor capabilities, the existing target-access owner, and repeated current-owner
mutation admission for VM phases. The original VM/epoch is pinned into native save/push so
a replacement between admission and RPC cannot inherit a stale request. Transactions are
released before native Git callbacks. Dispatcher admission permits concurrent modules, so
an agent turn does not exclusively lock out its requested `workspace.changes` callback.
Recorded per-repository provider outcomes remain normal G5 results; an outer provider
phase failure after the local commit returns an explicit partial-completion error with the
saved PR UUID and uncertain/not-attempted phase. Both clients expose the recovery message,
and UI PR lists refresh even on partial failure. Every fresh phase rechecks source policy
and target readability, including visibility changes during native preparation. VM write
POSTs do not automatically retry a lost response, since a retry could save newer edits even
though the PR record is reused.

## Target, title and GitHub ID amendment

The user explicitly requested implementation after the three compatibility gaps were
explained. The prior required-target handoff remains recorded in the read-only compatibility
audit in `review.md`; this amendment changes that decision before dependent code.

The shared PR owner resolves an omitted destination once, using readable live parent first,
then readable live Base workspaces ordered by name and ID in the same organization. It also
supplies the UI choices; remove the frontend's duplicate selection/filter policy. Explicit
target IDs continue through existing access checks. Missing defaults fail before saving or
publishing; access loss during later phases fails against the pinned target.

Add optional `title` to the shared request and `-t/--title` to create/update. Reject blank
titles before effects. Explicit titles update the reused or new G5 PR and its provider PRs;
omission preserves existing titles and keeps automatic derivation for new PRs. Reuse the
existing provider owner and partial-failure reporting rather than introducing a second flow.
Expose REST `node_id` as GitHub JSON `id`; retain G5 UUID semantics and numeric `number`.

Acceptance: omitted-target public and VM requests use the same default the UI displays;
parent preference, Base fallback, name/ID ordering, empty choices and private/deleted/cross-
organization exclusions are covered. Explicit overrides remain valid. Title create/reuse/
omission and provider failure reach persisted results; help/invalid fields and blank titles
perform no publication. Numeric and URL views accept `--json id` and return the node ID.

Design self-review: the system-invariant index leads back to current admission, durable read
authority and tenant boundaries. Default selection cannot broaden visibility or replace
target authorization. This is one owner plus its narrow read-only target-choice projection,
not another publication mechanism. Inspect existing visibility listings, title persistence
and provider update methods before relying on them. No unresolved intent blocks coding.

### Review amendment: durable title effects

Review exposed a cross-transaction title obligation absent from the initial design. Add a
nullable pending provider title to the existing G5 PR record, written atomically with an
explicit local title. The shared provider owner consumes current persisted intent under
existing admission and a PR row update lock, never a stale request-local string. Successful provider
synchronization clears the pending intent; partial failure retains it for title-less UI/CLI
retry. Once complete, omission still preserves existing provider titles. This small schema
addition supersedes the earlier no-schema expectation only for this title obligation.

Separate metadata-update eligibility from new-PR creation eligibility within that same
provider owner: recorded provider PRs for unchanged/included members receive pending titles,
without new empty PR creation or bypassing authority/readiness. Preserve known provider
identity and accurate failure/status evidence when PATCH fails. UI target choices participate
in existing workspace invalidation; there is no new event protocol. Acceptance adds failure/
retry, overlapping-intent, unchanged-member and mounted-hook cache-transition regressions.

Provider title serialization is owned by the G5 PR record: acquire its update lock before
loading pending intent and retain it through provider effects and result persistence. Local
title acceptance updates the same row. Source admission remains necessary but its public-
user shared lock is not an exclusive title fence; the PR lock supplies that serialization
without changing authorization or introducing a new orchestration.

## Automatic refresh after push amendment

The user approved automatic PR refresh after successful pushes, preserving version-bound
approval and commit checks during merge. Reuse this design and request lineage rather than
creating another plan. Existing PR identity and chosen target remain stable; manual
create/update continues to own explicit save/push preparation and metadata input.

The shared publication owner accepts an existing-PR continuation over committed workspace
state, triggered by successful source ref changes. Reuse its evidence, publication and
provider stages without calling back into the VM or duplicating policy in Git routes.
Keep incomplete work discoverable after process/provider failure and revisit eligible PRs
without starvation. An older notification reconciles current state instead of restoring
old captured heads. Current actor/tenant/target authorization applies at effect boundaries;
no historical pusher or PR creator gains permanent authority. Completed pushes stay successful
if the provider is unavailable; expose the pending/error state and recover without a manual
Update PR. Reads remain read-only.

Acceptance: create at A, push B, then observe the same open PR and provider record at B
and merge without an Update PR; source movement during merge still refuses stale intent.
Cover ordinary Git receive-pack and UI/CLI push convergence, local-only and provider PRs,
multi-member commits, rejected/no-op/obsolete pushes, closed PRs, absent PRs, current-owner
or target-access loss, provider failure/retry and restart discovery. Approval records never
advance with PR evidence. Tests use real API/DB/Git boundaries and fake only provider protocol.

Design self-review followed the invariant index back to separate Git/platform authorities,
current action admission and version-bound approvals. Automatic refresh changes the PR's
current published version, not an accepted merge's identity or a human's reviewed version.
The existing shared-flow commitment permits a narrower already-committed continuation;
automatic work must not inherit the explicit button's save-all effect. Reused queue, push,
publication, refresh and merge operations remain source-inspection assumptions for coding.
No unresolved product-intent question blocks the user's authorized implementation.

### Source-inspection clarification for automatic continuation

The provider create/update helper can push a source branch itself, and normal evidence
capture can read a stopped workspace's retained dirty snapshot. Automatic continuation
must use published branch refs and observe successful remote publication; it must not
inherit either of those effects. Add a narrow automatic mode to the same provider owner
that verifies the remote source head without pushing it. Existing remote integration
continues to own auto/manual push policy. Persist pending refresh/error on the existing
PR so a committed evidence update cannot lose provider retry work. Startup and periodic
scans recover missed branch events; events only reduce latency. Completed refresh emits
a PR update hint for source and target clients.

A provider merge and automatic refresh serialize on the PR record. Any recorded or
observed merged/closed provider PR prevents automatic repinning, preserving recovery of
an already accepted merge. No broader merge protocol redesign is included.

### Review repair decisions for automatic continuation

Every shared local evidence writer marks provider continuation pending when the captured
source/target vector changes; creation also starts pending. Only provider completion settles
it. Discovery additionally checks recorded provider source pins for upgrades from older state.
Read-only discovery determines whether work exists before effect admission. Automatic admission
uses GitOperation policy with current owner/transfer checks, since it saves no content.

Metadata refresh locks only its PR row; source/target identity locks precede that row, and
current target visibility/invitation is protected and access is rechecked
at effect admission after PR waits. The locked source owner and its same-organization
foreign key preserve membership; credentials recheck membership at each provider admission. This avoids workspace lock upgrades under the PR lock.
Known provider identity must match the current repository before observation. Existing provider
members are reconciled even when no new empty PR is eligible.

Automatic remote-ref observation uses the provider owner's bounded, cancellable GitHub HTTP
client and ContentsRead credentials rather than blocking libgit2 calls. It performs no Git
write and retains the same remote publication precondition. Separate pending/error fields are
projected and rendered independently of local merge readiness; accepted-merge recovery remains
available. These repairs implement the existing follow-push, authority and recovery commitments.

Automatic provider credential lookup reuses the caller's authorized tenant scope. The
credential owner still performs all membership/identity/binding/capability checks; it does
not acquire a nested tenant connection while retaining a PR lock. Ordinary callers retain
the existing transaction-free credential API. This avoids connection-pool/row-lock cycles
when another PR operation is waiting, without increasing pool capacity or weakening checks.

### Committed review repair decisions

Explicit UI/VM phases protect workspace identity in ordered locks and re-admit target
access after PR waits before publication or commit. Terminal title acceptance is refused.
Runtime identity is captured before tenant locking, then revalidated while locked.
Automatic work bounds lock waits and whole attempts so one entry cannot starve later work;
its durable inventory remains the retry authority after interruption.

Source dialogs consume current same-ID query state, with mutation fallback while a new PR
identity catches up. Successful mutation cancels older list reads and seeds cached matching
records. Ordering does not depend on database timestamps; terminal merge feedback remains
explicit. These are implementation repairs under the existing commitments; see the
[commit assessment](review.md#committed-change-review-f2b2d9f5--final-assessment).

## Workspace CLI usability implementation

The shared root harness directs platform-launched Claude and Codex agents to `g5d pr`
when G5 supplies workspace context. It owns the short routing and recovery guidance;
CLI help and `docs/g5d-pr.md` own detailed options. Keep `gh` available for explicit
external GitHub work. Main's harness delivery supplies the guidance on fresh and resumed
launches without provider-specific profiles or an extra injected prompt.

Share the G5-only launcher between agent and terminal startup. Supply it in the standard
guest command path and inherited PATH, preserving other image tools and existing image
directory modes. G5-owned command directories stay traversable across startup umasks.
Scope library settings to G5 processes; local fixtures must not change host command paths.

Terminals and agent.turn share the API URL, workspace ID and incarnation token under
the accepted MVP guest-token boundary. Reserved-key checks protect context; existing
server checks enforce current authority and one UI/CLI PR flow. No new proxy, credential
store or automatic write retry. Network-disabled Codex modes still cannot reach the API:
report the restriction and direct the owner to the workspace terminal without bypass.

Verify real terminal CLI/auth/error transport, launcher quoting/library isolation and both
provider harness consumers. Preserve main's limits, cancellation and execution custody.
Live model choice and deployed guest behavior are separate from fixture evidence.
