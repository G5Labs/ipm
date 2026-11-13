# GitHub PR authorship

## Ontology changes and governance

### Current amendment: PR checks for the VM agent

Design proposal for the [CI loop request](request.md#inspect-pr-checks-from-the-vm).
The user wants the agent to push, inspect CI and read failures before repairing its work;
direct gcloud/Cloud Build access is a separate project. This amendment adds read behavior
without changing the implemented publication/manual-retry contract. The user has authorized implementation.

**Ontology changes.** Comparison base: `21e0e65eaacf9596e3e80ca98af4585b49743d7f`.
Add `g5.governance.pr.checks`, `g5.governance.pr.checks.revision` and
`g5.governance.pr.checks.details` in [governance](../../ontology/features/governance.g5.md#inspect-provider-checks-from-a-workspace-agent),
and `g5.architecture.pr_checks` plus its public operation row in
[Git/governance architecture](../../ontology/software_architecture/git_governance.g5.md#one-provider-check-read-operation).
Each added block has paired metadata that records its unimplemented state. No existing
contract is weakened. The exact ontology diff is:

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 354e478e..b60498da 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -190,6 +190,55 @@ Governing: [workspace read authority](../concepts/security.g5.md#read-permission
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
+An optional expected head for one selected repository prevents old passing checks from
+satisfying a caller waiting for its push to reach GitHub. A mismatch is pending, not
+success. Watch re-resolves the provider head, discards superseded observations and
+revalidates it before reporting terminal results. A read is an observation at a time;
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
@@ -486,6 +535,24 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
 - [CLI PR retrieval request and scope selection](../../chat_intent/codex-pr-user-authorship/request.md). Command and repository selection details are agent design decisions.
 - Implemented in `g5d-cli/src/pr.rs`, `routes/{workspaces,internal}.rs`, incarnation auth middleware and `workspace_pull_request_read.rs`. Compiled CLI tests cover environment, config, stable metadata and explicit selection; restricted-role API tests cover user/VM admission, repository selection, narrow App and personal credentials, redirects and failures. Live GitHub and deployed guest execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#cli-retrieval-consolidated-discovery-before-repair).

+### g5.governance.pr.checks
+
+- human-verified
+- [VM agent CI loop request](../../chat_intent/codex-pr-user-authorship/request.md#inspect-pr-checks-from-the-vm). CLI syntax, multi-repository aggregation and observation limits are agent design decisions.
+- Unimplemented · the existing PR read interface does not expose CI checks. The current design adds live reads; provider integration and CLI behavior remain unverified.
+
+### g5.governance.pr.checks.revision
+
+- agent
+- [CI read design](../../chat_intent/codex-pr-user-authorship/ontology-plan.md#current-amendment-pr-checks-for-the-vm-agent) derives revision checks from the user's push/check/fix loop and existing publication and approval boundaries.
+- Unimplemented · expected-head selection, watch refresh and terminal-result revalidation need implementation and race verification.
+
+### g5.governance.pr.checks.details
+
+- human-verified
+- [Failure reading and Cloud Build exclusion](../../chat_intent/codex-pr-user-authorship/request.md#inspect-pr-checks-from-the-vm). Bounded GitHub output and link handling are agent design decisions.
+- Unimplemented · the PR read interface has no failure diagnostic projection; source inspection and provider/authorization tests remain necessary.
+
 ### g5.governance.pr.write

 - human-verified
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 89431f13..35008dce 100644
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
+- Unimplemented · the existing read service and credential owner are intended reuse points; their exact policy and permission support must be inspected during coding.
+
 ### g5.architecture.semantic_surfaces

 - agent
```

**Relevant and governing ontology.** The following commitment set governs coding and review.

| Governing IDs and home | Obligation and owner/operation | Allowed case / counterexample; conclusion |
|---|---|---|
| `g5.governance.pr.checks`, `.revision`, `.details` ([governance](../../ontology/features/governance.g5.md#inspect-provider-checks-from-a-workspace-agent)) | Shared PR read owner returns observed checks and scoped diagnostics with revision and availability evidence. | All linked providers can be inspected; old-head green, hidden pagination loss and unavailable data cannot establish success. New design; unimplemented. |
| `g5.governance.pr.read`, `.json` ([retrieval](../../ontology/features/governance.g5.md#retrieve-pull-requests-through-the-cli)) | Reuse selectors, authorized attached-repository resolution and validated field selection. | A current-workspace PR or explicit attached GitHub PR is readable; an arbitrary URL cannot expand access. Reused implementation policy needs inspection. |
| `g5.security.read`, `.tenant`, `.tenant.rls`, `.admission`, `.credentials.publication`, `.credentials.redaction` ([security](../../ontology/concepts/security.g5.md)) | Server admits each user/VM read, retains repository credentials and redacts public diagnostics. | Authorized durable reads do not require starting compute; stale VM credentials, cross-tenant IDs and provider links do not grant access. Preserve operation-specific read rules, not mutation-only admission. |
| `g5.governance.pr.write`, `.follow-push`, `.publish.readiness` ([publication](../../ontology/features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent)) | Existing publication owner alone saves/pushes and synchronizes PRs. | Agent publishes then reads; a checks request cannot silently repair a failed publication. No change to manual-retry behavior. |
| `g5.governance.approval`, `.approval.version`, `.merge` ([approval](../../ontology/features/governance.g5.md#approval-is-a-human-authority)) | Human approval and merge retain their own exact-version checks. | Passing observed checks support investigation; they do not approve or prove merge readiness. Preserved. |
| `g5.data.git-authority`, `.platform-authority.vm-access`, `.outcome-evidence` ([data](../../ontology/concepts/data_model.g5.md)); `g5.architecture.pr_checks` ([owner](../../ontology/software_architecture/git_governance.g5.md#one-provider-check-read-operation)) | API read owner keeps Git revision, G5 publication evidence and live provider outcomes distinct. | Provider data is read through the API; stored G5 head or successful push alone is not CI success. No new database state. |
| `g5.design.reuse`, `.minimal-mechanism`, `.adapters`, `.authority`, `g5.architecture.policy` ([principles](../../ontology/concepts/system_design_principles.g5.md)) | Extend one read owner; thin adapters share policy, retaining read/write differences. | CLI and later UI may consume one operation; no CLI-side provider credentials, duplicate check policy or server monitor. Reuse assumptions require source inspection. |
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

Open implementation assumptions: the current PR selector/read owner and credential resolver
are appropriate reuse points but their exact policy/effects have not been source-inspected
for this addition; inspect before reuse. Verify GitHub rerun identity, pagination and any
synthetic-merge revision behavior against protocol fixtures so no head is mislabeled.
The GitHub App's deployed permission grants and actual Cloud Build diagnostic payloads
remain unknown. None blocks this design or warrants inventing credentials/log access.
No implementation or runtime verification of this addition is claimed.
