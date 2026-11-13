# Tighten the Git abstraction

## Ontology changes and governance

**Comparison base:** `origin/main` at `fd2ad7ca`. The branch first rebased to
`093e08a7`, merged `8a3ec325` for PR 294, then integrated main's concurrent Git
simplification. Main's consolidated ontology, native-memory requirements, Base
tracking and landing/capture rules remain authoritative; execution and provider
mechanics remain inside SCM. Original reviewed ontology diffs are preserved at
`991f51ef` and `a2fb3d49`; the diff below records the combined design against
`fd2ad7ca`. Request and context:
[chat intent](chat_intent/design-git-abstraction/request.md).

**Ontology changes:**

- **Added in this branch:** eleven blocks in [Git/governance](ontology/software_architecture/git_governance.g5.md#git-execution-and-provider-access):
  `g5.architecture.git_execution`, `git_ref_updates`, `git_native_runner`,
  `git_outcomes`, `git_retry`, `git_credentials`, `git_locations`,
  `scm_provider_client`, `scm_credential_scopes`, `github_transport` and `git_guard`
  (all share the `g5.architecture.` prefix).
- **Modified:** `g5.architecture.git_owner` directs execution through the new layer
  while retaining its policy ownership. Its metadata qualifies inherited evidence;
  `g5.architecture.projection_owner` metadata records its typed-reader implementation and remaining verification limits. Implementation metadata covers typed operations, native CLI/lease/API-protocol/GC/proof execution and API-core/routes/workspace-ops backend closure, retaining unverified runtime boundaries. Metadata for `g5.architecture.workspace_registry`, `g5.architecture.content_reader` and `g5.data.git-authority` records scoped bootstrap/immutable-read evidence without changing their meaning.
- **Final backend metadata:** the contracts backend dependency/conversion is removed; SCM owns conversion and effective-identity executable lookup. All-backend dependency/native guards have calibrated exceptions. The account/login/PR/report provider migration is implemented and reviewed; scoped metadata and the final review ledger record its owner tests, calibrated guards, independent discoveries and repairs.
- **Guest increment metadata:** typed member/configuration/flush ownership, checkpoint isolation,
  flush working state/vector/identity/content/limits/exclusions, runtime custody and configured-secret
  redaction now record scoped runtime evidence and remaining interruption/VM boundaries.
- **Scoped durability metadata:** `g5.git.receipt`, `g5.git.receipt.write-admission`,
  `g5.git.network.cas` and `g5.git.network.ref-scope` record proof/push evidence and
  remaining concurrency/alternate-caller limits without changing their contracts.
  `g5.git.restore`, `g5.git.restore.cleanup` and `g5.git.restore.proof` record scoped
  host restore, exact-state, sequencer and missing-source evidence.
- **Scoped runtime metadata:** `g5.runtime.drain` records API fetch-stream cleanup and
  cancelled push awaiter/admission evidence, preserving the broader runtime race limits.
  `g5.runtime.materialization` records fresh host-root/control-file and cache evidence,
  retaining unexecuted KVM performance/eviction boundaries.
- **Scoped principle metadata:** `g5.design.minimal-api`, `g5.architecture.commands`,
  `g5.design.abstraction`, `g5.design.information-hiding`,
  `g5.architecture.dependencies` and `g5.architecture.compatibility` record the
  reviewed backend closure and caller/guard evidence, retaining their broader gaps.
- **Follow-up audit metadata:** `g5.fork.source`, `g5.fork.replay` and `g5.architecture.fork_owner` retain prior merge/fork evidence and mark combined landing/capture verification pending. Main's landing commit-point supersedes historical compensation evidence. Git ref/guard/outcome and existing principle/compatibility assessments record retired competing APIs, stronger structural checks and shared database-outcome classification; inherited receipt/provider/live limits remain explicit.
- **External-review follow-up:** clarify `git_native_runner` fetch execution budgets
  to exclude response-reader backpressure while retaining deadline and cancellation
  cleanup. Public HTTP outcomes omit server paths/stderr and preserve confirmed refs;
  provider errors distinguish refusal from availability. Host restore shares the cache's
  selected transport environment. SCM GitHub web configuration applies without an App,
  while App endpoint precedence remains. Scoped evidence belongs in the existing review.
- **Main integration:** preserve `g5.memory.repository` and `g5.git.exclusions`: SCM checks startup capture eligibility and ignored required paths using the HEAD-seeded temporary index. The adapter retains repository selection, memory paths and repair/retry messages. Keep main's repository-credential and Git-authority clarifications; move evidence for retired enforcement principles to `g5.architecture.commands`.
- **Main simplification integration:** `g5.git.landing.commit-point` and
  `g5.fork.source.cut` govern the single bounded capture reader and no post-COMMIT
  retraction. Preserve `g5.git.mirror`, `g5.workspace.remote.base-parity`,
  `g5.workspace.remote.base-provider-rewrite`, `g5.governance.remote.pull` and
  `g5.fork.source.advisory`; remove protected-ref import from `git_owner` and stale
  mirror/compensation metadata. Qualify combined runtime evidence as pending CI.
- **Second abstraction audit:** amend `g5.architecture.git_native_runner` (every
  remote-contacting operation runs through the runner; libgit2 is local-only) and
  `g5.architecture.git_credentials` (single native injection point; the incarnation
  relay authenticates only its own API hop). Metadata for `git_execution`,
  `git_ref_updates`, `git_locations`, `git_guard`, `scm_provider_client`,
  `scm_credential_scopes` and `github_transport` records the second-audit evidence
  and limits. [Design section](#second-abstraction-audit-repairs).
- **Navigation:** four new public-operation rows in Git/governance and the module-map
  row in [software architecture](ontology/concepts/software_architecture.md#module-map).
- The review amendment preserves the authorized content reader, defines backend-independent
  ref effects and operation-specific credential scopes, separates distinct repository
  layouts, and replaces count-based guards with ownership checks. All added blocks
  have paired metadata distinguishing audit reports from verified implementation.
- The user's retry constraint is authoritative: clear errors, agent/human retry decisions,
  no new tracking tables or other retry persistence. Existing feature, security,
  durability and recovery owners retain their contracts; this refactor does not add
  automatic replay or a new recovery service.

The following is the complete ontology diff against the comparison base, including metadata.

```diff
diff --git a/.g5/ontology/concepts/data_model.g5.md b/.g5/ontology/concepts/data_model.g5.md
index aaabb95f..1ba79f69 100644
--- a/.g5/ontology/concepts/data_model.g5.md
+++ b/.g5/ontology/concepts/data_model.g5.md
@@ -101 +101 @@ After scope resolution, apply [provider authority](../features/provider_accounts
-- ~75% last whole-block baseline · workspace_reader, workspace_flush_service and workspace_fork_service; migration 039 and workspace_repository_model remove cached heads. Complete docs-membership mapping audit is still missing and fork receipt selection races pin acquisition. Source inspection only; no whole-block remeasurement.
+- ~75% last whole-block baseline · workspace_reader, workspace_flush_service and workspace_fork_service; migration 039 and workspace_repository_model remove cached heads. Complete docs-membership mapping audit is still missing and fork receipt selection races pin acquisition. Scoped Git OID/timestamp and unpublished-candidate/checked-bootstrap API regressions pass; no whole-block remeasurement.
diff --git a/.g5/ontology/concepts/security.g5.md b/.g5/ontology/concepts/security.g5.md
index 7e5cca75..011c7bc8 100644
--- a/.g5/ontology/concepts/security.g5.md
+++ b/.g5/ontology/concepts/security.g5.md
@@ -285 +285 @@ This keeps [comment words](../features/documents.g5.md#keep-a-persons-words-when
-- ~60% · output redaction and native request ingestion; native semantics enter public metadata and redaction alters private payload bytes.
+- Partial · output redaction and native request ingestion; real flush RPC/log tests redact full configured tokens and bounded-output prefixes after native migration. Native semantics still enter public metadata and redaction alters private payload bytes; this scoped repair does not close those unrelated gaps. Provider migration additionally redacts selected merge-refusal tokens and full configured check secrets before message/header caps; real PR/check API projections and persisted errors are tested. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -321 +321 @@ This keeps [comment words](../features/documents.g5.md#keep-a-persons-words-when
-- ~75% · SCM provider adapters and repo_connection_service; full GitLab/local-only flows unassessed.
+- ~75% · SCM provider adapters and repo_connection_service; full GitLab/local-only flows unassessed. The SCM migration separates login and connection OAuth clients and verifies configured-instance import plus App/personal token admission; login does not gain repository authority. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
diff --git a/.g5/ontology/concepts/software_architecture.md b/.g5/ontology/concepts/software_architecture.md
index c0521c45..2b8ce6d1 100644
--- a/.g5/ontology/concepts/software_architecture.md
+++ b/.g5/ontology/concepts/software_architecture.md
@@ -43 +43 @@ scope when responsibility crosses a module boundary.
-| [Git/governance/documents](../software_architecture/git_governance.g5.md): ref effects, synchronization, approval and projections | `g5-api-core/src/services/{git_policy,remote_integrations,branch_commit,approval_review_service,doc_service,ontology_service,comment_service}`; `g5-api-core/src/auth/repository_credentials.rs`, projection maintenance; `g5-scm`, `g5-workspace-ops`, `g5-semantic` |
+| [Git/governance/documents](../software_architecture/git_governance.g5.md): ref effects, Git execution, provider clients, synchronization, approval and projections | `g5-api-core/src/services/{git_policy,remote_integrations,branch_commit,approval_review_service,doc_service,ontology_service,comment_service}`; `g5-api-core/src/auth/repository_credentials.rs`, projection maintenance; `g5-scm`, `g5-workspace-ops`, `g5-semantic` |
diff --git a/.g5/ontology/concepts/system_design_principles.g5.md b/.g5/ontology/concepts/system_design_principles.g5.md
index f1731d47..3e389e31 100644
--- a/.g5/ontology/concepts/system_design_principles.g5.md
+++ b/.g5/ontology/concepts/system_design_principles.g5.md
@@ -177 +177 @@ Provider fakes may replay recorded protocol but must not replace the production
-- ~60% · WorkspaceRuntime and typed lifecycle operations; raw task and workspace writers still bypass complete domain operations. Last whole-block baseline retained; clarified wording was not reassessed end to end.
+- Partial · WorkspaceRuntime and typed lifecycle operations; SCM exposes owned preparation/clone operations with raw handles removed, verified through caller suites and privacy checks. Raw task and workspace writers still bypass complete domain operations. Scoped provider clients expose named operations and owned outcomes without raw HTTP or new retry mechanisms; policy stays in existing owners. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). Follow-up Git audit retires unused writer/workflow exports and their sole semantic-injection port; existing semantic resolution and checked preparation remain. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
@@ -195 +195 @@ Provider fakes may replay recorded protocol but must not replace the production
-- ~75% · provider adapter/runtime/storage contracts hide concrete transports; provider-specific dispatch/config branches remain outside adapters, and complete change-locality and unnecessary-layer audit remains unassessed. Last whole-block baseline retained; clarified wording was not reassessed end to end.
+- Partial · provider adapter/runtime/storage contracts hide concrete transports; SCM owns Git preparation/backend handles and workspace-ops retains tested compatibility/composition. Provider-specific dispatch/config branches remain outside adapters; complete change-locality and unnecessary-layer audit remains unassessed. Git/provider mechanics are consolidated in SCM while API/host/guest layouts and credential/recovery policies retain their owners; the final review checks material consumers. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). The follow-up removes unused competing workflows and shares definite-COMMIT-refusal classification with ScopedDb instead of retaining two policies. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
@@ -201 +201 @@ Provider fakes may replay recorded protocol but must not replace the production
-- ~60% · domain DTOs and adapters; raw state writes and native semantic payload in public message metadata violate the intended boundary. Last whole-block baseline retained; clarified wording was not reassessed end to end.
+- Partial · domain DTOs and adapters; SCM backend handles/callback assembly are private, with external refusal and explicit public-signature/alias checks. Raw state writes and native semantic payload in public message metadata violate the intended boundary. Provider HTTP construction/delivery is private and scoped; public-operation privacy and backend/HTTP alias guards pass. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). Native command/child types, aliases and re-exports are covered by the explicit public-surface guard. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
diff --git a/.g5/ontology/features/agents.g5.md b/.g5/ontology/features/agents.g5.md
index 8277e5f7..f4453cdf 100644
--- a/.g5/ontology/features/agents.g5.md
+++ b/.g5/ontology/features/agents.g5.md
@@ -622 +622 @@ consolidation. Copying the memory directory does not remove these limits.
-- Prior assessments: ~90% binding, ~75% persistence · `g5d-agent-turn/src/memory.rs`, `g5d-dispatch/src/workspace_flush.rs`; native save → Git capture → fresh-home recall and binding checks passed. HEAD-tracked ignored memory survives startup/capture; ignored additions remain refused, including force-staged files. Edge-case gap: a detached writer surviving cancellation can write after Git capture (deliberate probe, not observed in ordinary auto-memory). Full microVM/interrupted capture and concurrent native writes remain unverified; [review evidence](../../../docs/experiments/agent_memory/review.md).
+- Prior assessments: ~90% binding, ~75% persistence · `g5d-agent-turn/src/memory.rs`, `g5d-dispatch/src/workspace_flush.rs`, `g5-scm/src/native/{checkout_queries,member/flush}.rs`; native save → Git capture → fresh-home recall and binding checks passed. HEAD-tracked ignored memory survives startup/capture; ignored additions remain refused, including force-staged files. Edge-case gap: a detached writer surviving cancellation can write after Git capture (deliberate probe, not observed in ordinary auto-memory). Merged SCM startup eligibility, real dispatcher capture/refusal/retry and host receipt restore regressions pass; the opt-in live Claude probe was not rerun. Full microVM/interrupted capture and concurrent native writes remain unverified; [review evidence](../../../docs/experiments/agent_memory/review.md).
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index c23b735c..8b38ea98 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -585 +585 @@ Automatic divergence pauses synchronization without launching resolution. An exp
-- Implemented in `g5d-cli/src/pr.rs`, `routes/{workspaces,internal}.rs`, incarnation auth middleware and `workspace_pull_request_read.rs`. Compiled CLI tests cover environment, config, stable metadata and explicit selection; restricted-role API tests cover user/VM admission, repository selection, narrow App and personal credentials, redirects and failures. Live GitHub and deployed guest execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#cli-retrieval-consolidated-discovery-before-repair).
+- Implemented in `g5d-cli/src/pr.rs`, `routes/{workspaces,internal}.rs`, incarnation auth middleware and `workspace_pull_request_read.rs`. Compiled CLI tests cover environment, config, stable metadata and explicit selection; restricted-role API tests cover user/VM admission, repository selection, narrow App and personal credentials, redirects and failures. Live GitHub and deployed guest execution remain unverified. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md#cli-retrieval-consolidated-discovery-before-repair). Current SCM migration tests additionally cover configured-instance attached identity and exact App/personal credentials with foreign-origin refusal. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -603 +603 @@ Automatic divergence pauses synchronization without launching resolution. An exp
-- Implemented · the checks owner retains and redacts GitHub output, bounds annotations/text, preserves partial output on retrieval failure, and stops requests on rate limits. API/CLI regressions verify partial diagnostics, 403/429, multi-repository fanout, secret/enum separation and external-link refusal. Actual Cloud Build payloads and direct logs remain unverified/outside scope.
+- Implemented · the checks owner retains and redacts GitHub output, bounds annotations/text, preserves partial output on retrieval failure, and stops requests on rate limits. API/CLI regressions verify partial diagnostics, 403/429, multi-repository fanout, secret/enum separation and external-link refusal. Actual Cloud Build payloads and direct logs remain unverified/outside scope. Current route regressions place configured secrets across provider diagnostic/header caps, including failed annotations; redaction precedes truncation and preserves retry evidence. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -615 +615 @@ Automatic divergence pauses synchronization without launching resolution. An exp
-- Implemented without new migrations in the shared PR owner, provider lifecycle and existing history/error projections. Real API/DB/Git tests verify successful push refresh, visible failures through local-only refresh, manual retry, missing-provider discovery, bounded progress and unchanged authority/approval checks. UI/cache tests verify current errors and recovery. Live GitHub, deployed guests and process restart remain unexecuted. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).
+- Implemented without new migrations in the shared PR owner, provider lifecycle and existing history/error projections. Real API/DB/Git tests verify successful push refresh, visible failures through local-only refresh, manual retry, missing-provider discovery, bounded progress and unchanged authority/approval checks. UI/cache tests verify current errors and recovery. Live GitHub, deployed guests and process restart remain unexecuted. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment). The SCM migration reruns actual automatic-refresh and manual recovery owners without changing persistence or authority; provider refusals/uncertainty retain useful errors. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -628 +628 @@ Automatic divergence pauses synchronization without launching resolution. An exp
-- Implemented with request-scoped title input and PR-row serialization/comparison. API tests with mocked GitHub cover explicit resubmission, omission, external edits, overlapping/stale requests, included-member updates and terminal refusal. UI/VM retries preserve completed provider pins and observed proof; arbitrary post-advance merge import is not claimed. Compiled CLI checks pass; live GitHub remains unexecuted. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).
+- Implemented with request-scoped title input and PR-row serialization/comparison. API tests with mocked GitHub cover explicit resubmission, omission, external edits, overlapping/stale requests, included-member updates and terminal refusal. UI/VM retries preserve completed provider pins and observed proof; arbitrary post-advance merge import is not claimed. Compiled CLI checks pass; live GitHub remains unexecuted. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment). Current provider migration tests preserve explicit resubmission and identity, including long create-error guidance through stored status/events and public output. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -682 +682 @@ Automatic divergence pauses synchronization without launching resolution. An exp
-- Implemented in `auth/repository_credentials.rs`, `services/github_user_grants.rs`, `services/github_app.rs` and `workspace_pull_request_provider_lifecycle.rs`. Restricted-role integration tests verify the actor token, refresh/refusal, binding identity, persisted connection recovery and existing-PR reuse; live GitHub authorship and the external permission matrix remain unexecuted. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md).
+- Implemented in `auth/repository_credentials.rs`, `services/github_user_grants.rs`, `services/github_app.rs` and `workspace_pull_request_provider_lifecycle.rs`. Restricted-role integration tests verify the actor token, refresh/refusal, binding identity, persisted connection recovery and existing-PR reuse; live GitHub authorship and the external permission matrix remain unexecuted. [Review evidence](../../chat_intent/codex-pr-user-authorship/review.md). Scoped SCM grants preserve this selection policy; the migrated API owner suite rechecks creation and existing-PR recovery with mock providers. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
diff --git a/.g5/ontology/features/platform_reports.g5.md b/.g5/ontology/features/platform_reports.g5.md
index 83b363ea..e1031b9d 100644
--- a/.g5/ontology/features/platform_reports.g5.md
+++ b/.g5/ontology/features/platform_reports.g5.md
@@ -36 +36 @@ Platform report intake fails closed when shared quota authority is unavailable.
-- ~90% · platform_issue_intake and report worker; external provider identity behavior unexecuted.
+- Implemented for inspected paths · Intake retains reporter attribution and server-side App credentials. SCM requests one configured report repository with issues/contents write and validates issue links and commit-pinned assets against that destination. Client and nine owner recovery tests pass; see the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). Actual external App attribution/permission behavior remains unexecuted.
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index 6f69543f..4100309d 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -730 +730 @@ This is the only reconciliation Base offers, because Base has no VM. It replaces
-- ~90% · workspace_fork_service operation replay; complete changed-input/concurrent replay matrix unexecuted. Source receipt pinning remains incomplete; repository-scoped resolution now uses the recorded requester, but interrupted credential-change/recovery cases remain unexecuted.
+- ~90% · workspace_fork_service operation replay; complete changed-input/concurrent replay matrix unexecuted. Source receipt pinning remains incomplete; repository-scoped resolution now uses the recorded requester, but interrupted credential-change/recovery cases remain unexecuted. Completed same-key replay after a concurrent source merge preserves the frozen manifest and child refs in branch and receipt-absent durable cases. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
@@ -790 +790,2 @@ This is the only reconciliation Base offers, because Base has no VM. It replaces
-- ~80% · workspace_fork_service reader/live paths; receipt selection and pin acquisition race pruning. Provider checks can select an owner connection instead of the recorded repository connection; complete session carry and concurrent capture remain unverified.
+- Partial · `workspace_fork_service::admit_operation` shares `read_committed_branch_heads` for source Git heads and receipt currency, under a short bounded fence released before provider/live work. Prior public merge/fork evidence is retained in the review; combined capture and pre-write refusal tests await CI under `g5.fork.source.cut` and `g5.git.landing.commit-point`. Receipt selection/pin acquisition still races pruning, repository-scoped credentials follow the recorded requester under the current credential contract, and credential-transition recovery plus full live/session/retained-receipt concurrency remain unverified. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
+
@@ -818 +819 @@ This is the only reconciliation Base offers, because Base has no VM. It replaces
-  The plan's reader inventory lists the acceptable unfenced readers. Not exercised: a tear between two members' ref writes. There is no hook between `apply_ref_commits` writes; the fence covers that window.
+  Combined Git abstraction verification awaits CI. The plan's reader inventory lists the acceptable unfenced readers. Not exercised: a tear between two members' ref writes. There is no hook between `apply_ref_commits` writes; the fence covers that window.
@@ -804 +804 @@ Other readers that persist or act on a cut also use the shared helpers:
-  - `read_committed_cut`: manifest, source download, clone-merge child.
+  - `read_committed_cut`: manifest, source download, clone-merge child; consumes the read-only admission transaction before reopening the same scope, avoiding nested pool acquisition. CI at `33fa4a7e` passes the constrained-pool public-reader regression and concurrent before/after merge and refused-landing fork cases; [follow-up evidence](../../ontology-review-git-abstraction.md#fourth-ci-follow-up--initial-findings). The internal (VM) manifest rechecks its incarnation after capture, because releasing admission also releases the incarnation row locks; `internal_manifest_refuses_an_incarnation_replaced_during_capture` covers a replacement mid-read ([review repairs](../../ontology-review-git-abstraction.md#code-review-repairs)).
diff --git a/.g5/ontology/software_architecture/boundaries.g5.md b/.g5/ontology/software_architecture/boundaries.g5.md
index f72e8205..ec5f7c36 100644
--- a/.g5/ontology/software_architecture/boundaries.g5.md
+++ b/.g5/ontology/software_architecture/boundaries.g5.md
@@ -56 +56 @@ policy or reversing dependency direction.
-- Historical ~50% estimate retained · `backend/g5-api-core/src/services`; JobService directly allocates task/session/input rows, and flush Git verification is route-owned before service completion; complete alternate-writer coverage remains missing. Consolidation retains the former boundary-enforcement scope here. `job_service.rs:720–793` still contains direct writes; this wording audit ran no application tests or whole-block reassessment.
+- Historical ~50% estimate retained · `backend/g5-api-core/src/services`; JobService directly allocates task/session/input rows, and flush Git verification is route-owned before service completion; complete alternate-writer coverage remains missing. Consolidation retains the former boundary-enforcement scope here. `job_service.rs:720–793` still contains direct writes; this wording audit ran no application tests or whole-block reassessment. Scoped Git enforcement: 32 calibrated architecture checks cover backend/native/provider and authorized-reader bypasses, including new SCM files, alternate calls and public process handles. Semantic duplication within allowed owners and arbitrary Rust macro/type mediation still require review. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
@@ -62 +62 @@ policy or reversing dependency direction.
-- ~100% of inspected manifest shape · selected reverse/sibling edges have source-contract checks; checks were not run here and do not establish semantic ownership or complete caller conformance.
+- Scoped manifest evidence · API-core/workspace-ops/CLI direct backend dependencies are dev-only, API-routes and semantic have none, and these boundaries are guarded; SCM native-only builds without libgit2. Foundation/host/runtime/worker/dispatcher/agent-turn normal dependencies exclude libgit2; workspace-ops path routing works with default features disabled. Contracts has no backend feature or dependency; only SCM has a production direct libgit2 edge. API/g5d aggregate direct backend use is dev-only, while the CLI intentionally retains transitive SCM backend linkage. Existing selected reverse/sibling source-contract checks were not rerun here and do not establish complete semantic ownership or caller conformance. Optional SCM providers do not enable native/libgit2 or depend on API/SQL. Whole-backend provider ownership and shared-transport guards pass. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -74 +74 @@ policy or reversing dependency direction.
-- ~100% of inspected facade placement · `backend/g5-api/src/lib.rs`, `g5d/src/lib.rs`; behavior behind every export was not revalidated.
+- Scoped facade evidence · `backend/g5-api/src/lib.rs`, `g5d/src/lib.rs`; workspace-ops re-exports SCM preparation and retains the legacy unused-worktree-argument wrapper without new policy. Runtime trust-file functions re-export SCM and workspace-ops keeps both existing path-routing exports. Affected unit/API/CLI/host checks pass; final backend conversion preserves the same owned error messages, with 232 SCM tests and remote-auth diagnostic regression evidence. API/core/g5d test targets compile and startup override/search/fallback tests pass; behavior behind every platform export was not revalidated. Final provider migration keeps existing API projections and owner recovery policies, with current auth/PR/check/report owner suites and test-target compilation. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). The abstraction audit removes unused internal merge/pull/mirror and semantic-injection APIs after a repository-wide caller inventory. Retained consumers compile and relevant CLI, workspace-ops and canonical semantic resolver suites pass. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
diff --git a/.g5/ontology/software_architecture/durability.g5.md b/.g5/ontology/software_architecture/durability.g5.md
index dba583a3..67bb4a32 100644
--- a/.g5/ontology/software_architecture/durability.g5.md
+++ b/.g5/ontology/software_architecture/durability.g5.md
@@ -231 +231 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- Implemented in shared selected-file helper and guest Git operation; live browser checks covered edited/new review files, four editor surfaces, no-op Save and durable forks.
+- Implemented in shared selected-file helper and SCM member checkpoint; real dispatcher tests preserve selected/new content, expected raw-byte refusal, unrelated staging and no-op behavior. Earlier browser evidence remains recorded in the issue design; this migration did not rerun the browser surfaces.
@@ -237 +237 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- Private-index selected commits and a native index lock preserve unrelated state; unit and live browser checks pass, but simultaneous native-index stress remains unexecuted.
+- SCM private index and real index lock preserve unrelated staging, including indexed attributes and literal newline paths in public/dispatcher tests. Dropped checkpoint cleanup preserves HEAD and permits a later native add. Simultaneous native-index stress and every ref/index compensation interruption remain unexecuted.
@@ -249 +249 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~90% last whole-block baseline · dispatcher flush temporary index; memory capture and refused-capture retry preserve the real index and HEAD. The broader dirty/sequencer/foreign-HEAD matrix remains unexecuted.
+- Scoped evidence · SCM temporary-index capture preserves separate branch, foreign HEAD and dirty snapshot in real shutdown flush tests; idempotent replay passes. Full sequencer/interruption matrix remains unexecuted. Migrated native-memory capture/refusal and host restore tests pass, preserving the real index and HEAD.
@@ -255 +255 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~85% last whole-block baseline · flush service/dispatcher/lifecycle; completed-turn memory capture passed, but the linked runtime drain defect can permit late native writes after cancellation. Full race/size matrix remains unexecuted.
+- Scoped evidence · dispatcher owns member vector, mutation drain and lifecycle custody; SCM owns Git capture/cleanup. Real noninterrupting code/docs fork and root Linux fork success/failure object handover pass. Full VM and lifecycle race matrix remain unexecuted. The inherited memory-specific detached-writer cancellation gap remains; completed-turn memory capture passes after migration.
@@ -261 +261 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~85% · flush service/dispatcher/lifecycle; race/size matrix unexecuted.
+- Scoped evidence · existing dispatcher capture state freezes receipt/vector and SCM publishes captured OIDs; real dirty shutdown flush replay returns the same complete result. Full partial-leaf/lost-reply matrix remains unexecuted.
@@ -267 +267 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~85% last whole-block baseline · flush service/dispatcher/lifecycle; native memory is captured with HEAD ancestry through the temporary index. Staged and unstaged ignored topics fail recoverably; already committed ignored memory remains captured. Broader race/size matrix unexecuted.
+- Scoped evidence · SCM temporary-index snapshot and bounded pack measurement keep mapped branch and checkout ancestry separate; real foreign-HEAD/dirty flush and root Linux handover pass. Every pack-limit/cancellation combination remains unexecuted. Native memory remains eligible; ignored additions, including force-staged paths absent from HEAD, must fail recoverably while committed ignored content remains captured. The migrated dispatcher regressions pass for these cases.
@@ -273 +273 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~90% · workspace_flush_service and route verification; public complete service method depends on caller verification and all alternate callers are unassessed.
+- ~90% · workspace_flush_service and native SCM route verification. Public API checks cover exact leaves versus shadow refs, missing/noncommit/wrong-width proofs, reversed ancestry, corrupt storage and ancestor fallback; refusal preserves completion/selection and a restored valid proof completes. The public complete service method still depends on caller verification; alternate callers and receipt pin/prune races remain unassessed.
@@ -279 +279 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- Unassessed · flush receive-pack admission and retry/refusal cases were not inspected in this documentary repair.
+- Partial · API incarnation push matrix verifies scoped branch/immutable flush writes, unknown receipt and changed-leaf refusal; real post-receive failure injection preserves confirmed branch signals while rejected leaves remain absent. Public-route cancellation holds the workspace lock through native completion and preserves confirmed continuation. Complete receipt/finalization races remain unverified.
@@ -285 +285 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~75% · flush API/dispatcher limits; exact transport/cap agreement partly unassessed.
+- Partial · dispatcher retains file/tree limits and SCM stops pack measurement at the owner-supplied bound. Existing limits/defaults are unchanged; exact large transport/cap and all cancellation boundaries remain unexecuted.
@@ -333 +333 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~80% · VM selection/host materialization; adversarial restore unexecuted.
+- Scoped evidence · host spec/receipt selection calls `g5-scm::native::restore_checkout` after fresh materialization. Host tests and independent real-Git probes preserve old mapped tip, foreign/detached HEAD and large binary unstaged content despite newer fetched tracking refs. Full VM boot and the complete warm/cold failure matrix remain unexecuted. [Review evidence](../../ontology-review-git-abstraction.md).
@@ -339 +339 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~80% · VM selection/host materialization; adversarial restore unexecuted.
+- Scoped evidence · SCM restore clears sequencers, resets the selected exact head, cleans nonignored extras and reapplies the binary snapshot without automatically merging newer heads. Host restore and independent attached/detached/binary/sequencer probes pass; interruption after each destructive stage is not separately fault-injected. [Review evidence](../../ontology-review-git-abstraction.md).
@@ -345 +345 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~80% · VM selection/host materialization; adversarial restore unexecuted.
+- Scoped evidence · SCM fetches required receipt leaves and checks exact OIDs/commit types; cached refs/objects do not satisfy a missing remote receipt in independent real-Git probes. Wrong repository-format OIDs refuse before tracking writes. Full remote-race combinations remain unexecuted. [Review evidence](../../ontology-review-git-abstraction.md).
@@ -363 +363 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~90% · Git receive policy and ref transaction; complete concurrent-update matrix unexecuted.
+- ~90% · Git receive policy and ref transaction; API push regressions preserve exact leased rewrites, stale server-CAS refusal and absent-branch refusal through typed observation migration. Complete concurrent-update matrix remains unexecuted.
@@ -376,0 +377,2 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
+  Combined Git abstraction runtime verification awaits CI.
+
@@ -387 +389 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~90% · Git receive policy; exhaustive principal/ref matrix unexecuted.
+- ~90% · Git receive policy; public user/incarnation push matrix and symbolic-workspace-ref refusal pass after native observation migration. Exhaustive principal/ref matrix remains unexecuted.
@@ -405 +407 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
-- ~75% last whole-block baseline · host materialization/session files; shared-memory Git capture/restore, private session/state exclusion and ignored-topic refusal/retry passed. `g5d-dispatch/src/workspace_flush.rs` checks actual ignored memory against its capture index, including topics staged in the real index. Committed ignored content remains captured. Complete native Git/flush exclusion coverage remains unverified.
+- Partial · SCM installs owner-supplied member identity, trust and excludes; native add/commit and dispatcher flush regressions pass. Native-memory ignored-path validation moves with temporary-index capture into `g5-scm/src/native/member/flush.rs`; shared-memory capture/restore, private-state exclusion and refusal/retry tests pass on this merged checkout. Committed ignored memory stays eligible. Complete native Git/flush exclusion coverage remains unverified.
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 30dbf80e..c11e36cb 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -28,0 +29,4 @@ and [governance](../features/governance.g5.md); post-MVP governance retains its
+| Authorized workspace reader | File/tree/diff/history over the selected authorized repository view | [Content reader](workspace.g5.md#durable-repository-reads) |
+| `g5-scm` | Typed object, diff and ref operations; native Git protocols, materialization, gc and member setup; credential injection | [Execution layer](#one-git-execution-layer), [ref effects](#keep-ref-effects-behind-typed-git-operations), [outcomes](#report-what-a-git-or-provider-operation-established) |
+| Repository-provider clients | Typed pull-request, merge, issue, installation, connection-validation and organization-listing operations per provider | [Provider HTTP](#one-client-per-repository-provider) |
+| GitHub transport | Configured base URLs, headers, rate-limit evidence and error mapping for provider, login and bug-report clients | [Shared transport](#share-one-github-http-transport) |
@@ -37 +41,2 @@ Git owners expose checked receive-pack, publication and merge
-operations over reusable `RepoStorage`, `GitRepo` and workspace Git mechanisms.
+operations over reusable `RepoStorage`, `GitRepo` and workspace Git mechanisms, all
+executed through the [Git execution layer](#one-git-execution-layer).
@@ -133 +138 @@ Applies [complete operations](boundaries.g5.md#keep-complete-operations-with-the
-and [entry-path enforcement](../concepts/system_design_principles.g5.md#enforce-boundaries-on-every-entry-path);
+and [entry-path enforcement](boundaries.g5.md#keep-complete-operations-with-their-state-owner);
@@ -142,0 +148,200 @@ synchronization to API maintenance through shared typed Git readers.
+## Git execution and provider access
+
+### One Git execution layer
+
+[D · g5.architecture.git_execution]
+
+`g5-scm` is the only platform code that opens repositories, links libgit2 or starts the
+`git` program. Its public operations use owned Git inputs and outcomes, keeping backend
+handles, command arguments and environment assembly private. Policy owners compose
+complete operations without their own libgit2 access; entry adapters call those owners.
+The native and libgit2 implementations share backend-independent types, and native-only
+consumers need not link libgit2.
+
+Application content consumers retain the [authorized reader](workspace.g5.md#durable-repository-reads),
+which uses this layer without relinquishing source selection, pinning or read authority.
+Projection maintenance uses its declared [typed-reader path](#convergent-projection-owner).
+Moving a raw read into `g5-scm` does not authorize a caller to bypass either owner.
+
+This hides the libgit2-versus-native variation ([stable abstractions](../concepts/system_design_principles.g5.md#create-abstractions-around-stable-meaning))
+and gives credentials, environment and ref safety one home. Git that users and agents run
+interactively in the guest is their own tool ([familiar Git](../features/workspaces.g5.md#use-familiar-git-tools)), not platform code.
+
+<a id="change-refs-only-through-g5-scm-transactions"></a>
+### Keep ref effects behind typed Git operations
+
+[D · g5.architecture.git_ref_updates]
+
+Every platform ref effect belongs to a typed `g5-scm` operation that declares the
+repository, affected ref scope, expected state and whether its updates form one
+transaction or can succeed separately. The calling policy owner supplies authority;
+native fetch/receive-pack and libgit2 transactions both implement this contract.
+Moving native execution into the crate is not itself a ref-safety check.
+
+Authoritative writes retain their [CAS](durability.g5.md#update-mapped-branches-through-exact-old-oid-cas),
+[ref-scope](durability.g5.md#restrict-network-ref-effects), [flush admission](durability.g5.md#admit-flush-writes-only-for-their-unfinished-receipt)
+and [publication proof](../features/governance.g5.md#prove-initial-external-publication)
+rules. Creating a run-specific resolution pair requires absent refs or identical
+existing values; replacing it requires the expected current values and one checked
+multi-ref transaction. A stale precondition refuses its transaction before mutation;
+independently admitted protocol transactions may still succeed.
+Local cache/tracking refs and checkout HEAD retain their materialization rules rather
+than acquiring mapped-branch authority. Outcomes identify accepted, refused and
+uncertain effects; post-acceptance signals contain only confirmed changes.
+
+### Run native Git through one runner
+
+[D · g5.architecture.git_native_runner]
+
+Where native Git is needed (network transport, receive-pack and pack-objects, gc, guest
+checkout and restore), `g5-scm` runs it through one runner. The runner resolves the
+binary once, applies the declared environment policy of its context (API, host, guest
+dispatcher or CLI) and returns typed errors. It is usable without linking libgit2, so
+host and guest binaries can depend on it alone.
+
+The runner honors the calling operation's deadline and owns child and stream cleanup
+through completion or cancellation. A fetch execution budget excludes waits to deliver
+bytes to its response reader, but one delivery blocked for the whole budget ends the
+execution; dropping that reader still cancels and reaps Git.
+Cancellation or process failure does not establish
+that remote or already accepted ref effects were undone; report the
+[established outcome](#report-what-a-git-or-provider-operation-established).
+
+Platform operations never write global or system Git config. Installing persistent member
+configuration and the owner's author identity ([shared native identity](durability.g5.md#keep-platform-artifacts-outside-native-git))
+are declared runner operations; other settings, such as a flush's temporary index, apply
+only to a single invocation.
+Host materialization keeps [ignoring retained guest control files](runtime.g5.md#host-git-never-trusts-retained-guest-control-files).
+The guest context takes its command path and loader environment from the
+[workspace CLI runtime setup](runtime.g5.md#make-the-workspace-cli-available-to-people-and-agents)
+rather than composing its own.
+
+### Report what a Git or provider operation established
+
+[D · g5.architecture.git_outcomes]
+
+Git and provider operations return typed, redacted outcomes distinguishing refusal
+before effects, confirmed completion, partial completion and an unknown result.
+Errors identify the operation and target, known completed effects, what failed or
+remains unknown, and useful corrective action or provider retry-after information.
+Neither timeout nor a lost response proves that a write did not happen. Existing
+read/view operations let the caller inspect current state before deciding to retry.
+
+Governing: [authoritative outcomes](../concepts/data_model.g5.md#report-outcomes-from-their-authoritative-owners),
+[recovery ownership](../concepts/system_design_principles.g5.md#make-failures-and-retries-explicit)
+and [secret redaction](../concepts/security.g5.md#redact-secrets-from-public-output).
+
+### Leave new retry decisions to the agent or human
+
+[I · g5.architecture.git_retry]
+
+This Git/provider abstraction adds no tracking tables, retry columns, queues, journals
+or background retry workers. It reports clear errors and does not automatically replay
+failed operations or sleep and retry rate-limited requests; the agent or human decides
+how and when to retry through the existing operation, with current authorization and
+preconditions. An unknown write outcome directs the caller to inspect existing state
+before reissuing it, without promising exactly-once execution.
+
+Existing independently governed durability, lifecycle and outbox recovery stays with
+its owners; the abstraction adds no persistence or recovery authority. PR follow-push
+failures use the existing [explicit Update PR flow](../features/governance.g5.md#follow-successful-source-pushes-automatically).
+
+### Inject Git credentials in one place
+
+[D · g5.architecture.git_credentials]
+
+Only `g5-scm` turns a credential into an authenticated libgit2 or native Git operation.
+It supplies the secret per invocation, never in process arguments, persisted config or
+remote URLs, and [redacts](../concepts/security.g5.md#redact-secrets-from-public-output) it from errors.
+
+Which credential a context may hold stays with security: guest code uses only the
+[incarnation token](../concepts/security.g5.md#expose-the-incarnation-token-under-the-mvp-boundary),
+and [provider publication tokens stay server-side](../concepts/security.g5.md#keep-provider-publication-credentials-server-side).
+
+### Resolve repository locations from their owners
+
+[D · g5.architecture.git_locations]
+
+Repository locations come from their semantic owners: `RepoStorage` for API storage,
+host cache/materialization for retained objects, guest layout for member checkouts and
+the development-container owner for its mirror. Shared location values preserve those
+kinds; a dev mirror is not an API repository root. Callers do not reconstruct another
+owner's roots or URLs. Bare-versus-worktree Git-directory resolution belongs inside
+the execution adapter and may consume an owner-supplied path. Share layout mechanics
+only where the location meaning is the same.
+
+### One client per repository provider
+
+[D · g5.architecture.scm_provider_client]
+
+Each repository provider (GitHub, GitLab) has one client behind `ScmProvider` that owns its
+provider operations, pagination and error meaning. Pull-request reads and lifecycle,
+merges, external issues, App installation binding, user repository-access checks,
+SCM-connection validation, organization/group listing and remote integrations request
+its typed operations; no caller receives its raw HTTP client or branches on the provider
+name itself.
+
+Clients consume [credentials scoped to the requested operation](#match-provider-credentials-to-the-operation-scope).
+They own provider protocol and provider-specific errors; account, installation-binding
+and publication policy remains with their existing owners. Provider selection belongs
+at the composition boundary, not in each consumer.
+
+Two owners stay outside it because their credential and authority differ: OAuth login
+identity (authorize, token exchange, user and email lookup) in
+[AuthService](platform.g5.md#login-identity-and-repository-connection), and platform bug
+reports in [`PlatformIssueGithubClient`](platform.g5.md#durable-platform-bug-report-intake).
+Calls that use a stored SCM connection token are provider operations, even when
+AuthService exposes them.
+
+### Match provider credentials to the operation scope
+
+[D · g5.architecture.scm_credential_scopes]
+
+Connection validation accepts the candidate credential under AuthService authority;
+account/organization discovery accepts that user's SCM connection; installation
+discovery and token issuance accept the App or installation grant selected by their
+owner. These operations require no invented repository identity. Repository operations
+consume the credential selected by `repository_credentials` for that operation,
+preserving [user PR authorship](../features/governance.g5.md#attribute-github-pr-creation-to-the-initiating-user).
+
+Distinct typed inputs identify the scope, provider host and intended operation. The
+provider client executes with the supplied grant; it never looks up, substitutes or
+falls back to another actor's grant. Installation-token issuance is an explicit
+provider operation used by credential selection, not a recursive call back into it.
+Any existing credential lifetime/cache policy remains with the credential owner.
+
+### Share one GitHub HTTP transport
+
+[D · g5.architecture.github_transport]
+
+The GitHub provider client, OAuth login and platform bug reports send GitHub requests
+through one transport implementation. It owns standard headers, rate-limit evidence,
+transport errors and URL construction and parsing from a configured web and API base,
+and never forwards credentials across redirects. Typed clients retain endpoint-specific
+request formats and provider meaning. Rate-limit responses expose retry-after evidence
+under [caller-decided retry](#leave-new-retry-decisions-to-the-agent-or-human).
+
+Each owner configures its own GitHub host, because the repository provider (possibly
+GitHub Enterprise), login and G5's own bug-report repository can differ. Within an owner,
+every request and every accepted PR or issue URL uses that host. Each caller supplies its
+own credential and keeps its own typed operations; the transport never selects or stores
+credentials, so sharing it confers no
+[login-to-repository authority](../concepts/security.g5.md#keep-platform-login-distinct-from-repository-integration).
+
+### Guard against new Git bypasses
+
+[D · g5.architecture.git_guard]
+
+Architecture source contracts and visibility rules reject backend handles or generic
+command/HTTP escape hatches in public APIs, production `git2` outside `g5-scm`, Git
+spawns outside the runner, application reads bypassing the authorized reader and
+GitHub requests bypassing typed clients or their shared transport even through a
+configured URL. Provider dispatch is permitted at composition only. Host-literal
+checks supplement these ownership checks; they are not proof of mediation.
+
+Extend existing reader, remote-integration and dependency guards, retaining their
+legitimate owner paths. Production allowlists identify operations, not whole files,
+and may only shrink during migration; tests and fixtures are exempt from production
+dependency checks. Fixtures must reject bypasses, including when the expected target
+of a check disappears, and accept harmless internal refactors. Guards follow
+[boundary evidence](../concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence).
+
@@ -149 +354 @@ synchronization to API maintenance through shared typed Git readers.
-- ~75% last whole-block baseline · `backend/g5-api-routes/src/routes/git.rs`, `g5-api-core/src/services/git_policy.rs`, `g5-scm`, `g5-workspace-ops`; repository-scoped credential selection now exists in sampled fork/mirror/integration callers. Equal-OID publication adoption remains a recorded proof gap; exhaustive credential callers remain unassessed and object-closure isolation is post-MVP.
+- Partial · `backend/g5-api-routes/src/routes/git.rs`, `g5-api-core/src/services/git_policy.rs`, `g5-scm`, `g5-workspace-ops` retain the policy operations. API routes now delegate observations and smart-HTTP execution to typed SCM operations while retaining authentication, exact-ref policy, SQL admission and publication continuation. Public push/flush and cancellation/failure regressions pass. Repository-scoped credential selection exists in sampled fork/integration callers. Equal-OID publication adoption remains a recorded proof gap; exhaustive credential callers remain unassessed and object-closure isolation is post-MVP.
@@ -155 +360 @@ synchronization to API maintenance through shared typed Git readers.
-- Source-inspected · `auth/repository_credentials.rs`; fork, mirror and integration worker use the resolver. Exhaustive PR/adoption/transport caller audit remains unexecuted.
+- Source-inspected · `auth/repository_credentials.rs`; fork and integration worker use the resolver. Exhaustive PR/adoption/transport caller audit remains unexecuted.
@@ -167 +372 @@ synchronization to API maintenance through shared typed Git readers.
-- Implemented in the existing shared service, provider lifecycle, bounded maintenance and UI/CLI adapters. Full-flow outcomes reuse PR events; local preflight cannot settle provider failure, and missing-provider discovery shares synchronization eligibility. Main's migration directory is unchanged. API, CLI and UI checks cover manual recovery, request-scoped titles, current admission and preserved completed-provider evidence. Live provider/guest execution remains unverified. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment).
+- Implemented in the existing shared service, provider lifecycle, bounded maintenance and UI/CLI adapters. Full-flow outcomes reuse PR events; local preflight cannot settle provider failure, and missing-provider discovery shares synchronization eligibility. Main's migration directory is unchanged. API, CLI and UI checks cover manual recovery, request-scoped titles, current admission and preserved completed-provider evidence. Live provider/guest execution remains unverified. [Manual retry assessment](../../chat_intent/codex-pr-user-authorship/review.md#manual-retry-final-assessment). The final SCM migration preserves these owner policies; current API/database/Git fixtures recheck explicit recovery, title resubmission, provider acceptance and publication pins, with bounded unknown-outcome evidence. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -173 +378 @@ synchronization to API maintenance through shared typed Git readers.
-- Implemented · `workspace_pull_request_checks.rs` reuses attached-repository selection, current user/VM admission, effective secret configuration and `RepositoryOperation::ReadPullRequestChecks`; the credential owner grants only Pull requests/Checks/Commit statuses read. API/static and existing publication regressions pass. No new database state or CI mutation. [Checks evidence](../../chat_intent/codex-pr-user-authorship/review.md#pr-checks-final-assessment).
+- Implemented · `workspace_pull_request_checks.rs` reuses attached-repository selection, current user/VM admission, effective secret configuration and `RepositoryOperation::ReadPullRequestChecks`; the credential owner grants only Pull requests/Checks/Commit statuses read. API/static and existing publication regressions pass. No new database state or CI mutation. [Checks evidence](../../chat_intent/codex-pr-user-authorship/review.md#pr-checks-final-assessment). Protocol now lives in SCM; the API retains admission, credential selection and configured secrets. Current route tests cover configured instances and foreign origins, revision consistency and annotation-error redaction before truncation. [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
@@ -203 +408,74 @@ synchronization to API maintenance through shared typed Git readers.
-- ~90% of inspected convergence mechanism · `backend/g5-api-core/src/maintenance/projection_convergence.rs`; startup/60-second polling and OID checkpoints exist, but every reader's lag guard and crash/rebuild behavior were not verified.
+- Partial · `backend/g5-api-core/src/maintenance/projection_convergence.rs` now discovers refs through `g5-scm::GitRepo::{head_ref_commits,ref_commit_oid}` while retaining storage selection, polling and OID checkpoints. Shared-reader tests distinguish absent refs, invalid targets and missing mirrors; all eight existing projection-convergence integration tests pass. Exhaustive crash/rebuild coverage is not claimed.
+
+### g5.architecture.git_execution
+
+- agent
+- [Design request](../../chat_intent/design-git-abstraction/request.md), [qualified audit evidence](../../ontology-plan-git-abstraction.md#evidence-and-scope) and design review distinguish consolidation from an existing ownership violation.
+- Implemented for the approved migration · `g5-scm` owns production libgit2 and native Git mechanics across API/protocol/GC, workspace-ops, CLI, host and guest consumers; only SCM has a production direct backend dependency. Contracts have no backend feature or conversion, native-only graphs exclude libgit2, and raw handles/callbacks are private. Provider protocol now uses scoped SCM clients, preserving the existing policy owners. Behavioral, dependency and calibrated boundary evidence, with native runtime limits, is retained in the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). The follow-up audit removes unused effectful merge/pull/mirror, semantic-injection and unchecked ref APIs; retained callers use typed checked operations. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
+
+
+### g5.architecture.git_ref_updates
+
+- agent
+- [Design](../../ontology-plan-git-abstraction.md).
+- Partial · `g5-scm/src/resolution.rs` checks both direct refs under both locks; existing provider fences authorize abandoned preparation. Native CLI push and `GitRepo` leased branch/commit publication and deletion preserve caller policy, exact old-head inputs and single-ref scope. Local/TLS remote-OID and API/database publication regressions pass. An already-equal destination can still return unchanged for an absent-head lease; this does not prove first publication. Local-repository setup configures symbolic HEAD; SCM exact/initial preparation validates batches before object writes and returns unattached candidates, preserving refs, index and checkout. Checked org bootstrap still owns branch publication; its API regressions pass. API receive-pack consumes protocol-confirmed acceptance before optional reads; real push tests preserve queue/events through a rejected sibling and failed observation. Unknown unreported effects emit no confirmed signal. SCM receive validates the admitted command vector and preserves native atomic/per-ref behavior. Bare/worktree, stale-CAS, partial acceptance and scope-mismatch tests pass. Host full-branch cache refresh/prune and receipt tracking/checkout effects are typed; exact pinning, wrong-object-format refusal before fetch and mapped-versus-foreign/detached state have runtime evidence. Guest checkpoints use exact branch CAS with index compensation; leased pushes retain captured OIDs, and flush writes declare two independently confirmed immutable leaves. Real member/flush regressions pass; the complete partial-leaf cancellation matrix is unexecuted. Fork source capture uses the shared fenced reader. Main's landing commit-point contract supersedes post-COMMIT compensation: deferred checks precede ref writes, partial Git writes restore under the same fence, and accepted cuts are retained after a COMMIT failure. Combined fork/merge and refusal regressions await CI. CLI alignment retains pre-capture expectations and refuses intervening changes. Unresolved Git restoration remains a partial-outcome boundary with explicit ref evidence. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
+
+
+### g5.architecture.git_native_runner
+
+- agent
+- [Design](../../ontology-plan-git-abstraction.md).
+- Partial · `g5-scm/src/native` owns API GC, CLI materialization/push and `GitRepo` lease push/delete, executable lookup, context environment, bounded output and child/credential cleanup. API GC clears inherited controls; its sweep awaits cleanup before unlock by source inspection, with DB child/lock-contender checks. Native lifecycle tests pass on macOS/Linux; CLI signal tests pass on macOS. API outer-timeout cancellation through existing blocking wrappers remains unverified. API admission/flush observations use anchored native reads with supervised stdin, per-invocation deadlines and typed errors; native-only SHA-1/SHA-256, corruption and cleanup checks pass on macOS/Linux. SCM also owns API advertisement, buffered receive and bounded fetch streams. Native tests preserve large binary data and error EOF, deadlines and body-drop cleanup; an aborted public-route awaiter retains SQL admission through Git completion, then confirms refs/queue/events. CLI version/default-branch/HEAD/ignore/history and mirror operations use this runner; parsed Doctor/Spec/Job/Push process-entry tests verify SIGINT/SIGTERM cleanup before exit. CLI materialization no longer writes user/global/system trust config, with actual Add/materialization config-preservation evidence. Host cache/restore use explicit anchored paths and selected host config/SSH/proxy/CA/loader context. A dropped public cache awaiter retains both locks through child cleanup. Runtime exact-root trust-file mechanics delegate to SCM. Native suites pass on macOS/Linux, and host normal dependencies exclude libgit2. Dispatcher member execution/configuration uses typed SCM operations and runtime-selected context. Owned custody retains admission, leases and object handover through cleanup; dropped checkpoint and root Linux fork success/failure regressions pass. Full VM and all interruption stages remain unexecuted. API startup delegates PATH lookup to SCM with effective-identity access checks; nonroot permission, override and fallback regression tests pass. External-review repairs exclude HTTP-reader backpressure from the fetch execution budget and reuse the cache environment for host restore; 66 native tests and both ordinary/required-config real restore tests pass. Live proxy/CA/SSH coverage remains unchanged. See [follow-up evidence](../../ontology-review-git-abstraction.md#external-review-repairs-and-verification).
+
+
+### g5.architecture.git_outcomes
+
+- agent
+- [Validation and retry clarification](../../chat_intent/design-git-abstraction/request.md#validation-and-retry-clarification) and [design](../../ontology-plan-git-abstraction.md#errors-and-caller-decided-retry).
+- Implemented with scoped evidence · Git/native/provider outcomes preserve refusal, confirmed effects, partial completion and uncertainty. Existing API/CLI/host/guest surfaces retain redacted errors and inspect-before-retry guidance. Provider errors retain observed status, request and retry evidence through body failure; checks scrub owner secrets before diagnostic caps, semantic merge refusals are typed, and bounded PR status/events preserve outcome guidance. Real API/Git and mocked-provider owner regressions are recorded in the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). Admission-release failure, full restore partial-stage and every flush cancellation point remain unexecuted. The landing owner follows `g5.git.landing.commit-point`: accepted Git cuts remain authoritative after a database COMMIT failure; incomplete under-fence Git restoration reports unresolved refs and inspection-before-retry guidance. Combined-owner verification awaits CI. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment). External-review HTTP projections retain operation/effect and confirmed refs while keeping server paths/stderr private; route tests pass. Provider kind/status mapping is covered by a complete kind/effect matrix; revised API expectations await CI. See [follow-up evidence](../../ontology-review-git-abstraction.md#external-review-repairs-and-verification).
+
+
+### g5.architecture.git_retry
+
+- human-verified
+- [User's retry constraint](../../chat_intent/design-git-abstraction/request.md#validation-and-retry-clarification); the design applies it to the new abstraction without replacing independently governed recovery.
+- Implemented · The complete migration adds no tracking tables, retry columns, queues, journals, workers or failed-operation replay. Native and provider writes execute once per invocation; unknown results direct inspection. Existing host visibility recovery, fixed-OID guest capture, PR reconciliation/manual Update PR and platform-report outbox remain with their independently governed owners. Persistence, publication and recovery regressions and limits are in the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation).
+
+### g5.architecture.git_credentials
+
+- agent
+- [Design](../../ontology-plan-git-abstraction.md).
+- Implemented for migrated operations · SCM alone injects Git credentials per invocation; native/libgit2 destinations recheck the selected provider, with redirects refused and public diagnostics redacted. Configured GitHub instances carry HTTPS origin, port and path prefix through App/personal resolution; sibling/dot/escaped paths cannot receive credentials. Guest Git uses the existing in-memory incarnation relay, and provider grants remain server-side. API import/resolver/read tests and native HTTP/TLS/askpass evidence are in the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). Full encrypted-key SSH handshake and remote shallow/SSH clone coverage remain unexecuted. A macOS combined native-then-libgit2 TLS test fails before ClientHello while its isolated run passes; transport-order behavior remains unresolved, with unchanged origin assertions. [Current audit limit](../../ontology-review-git-abstraction.md#remaining-macos-scm-test-order-failure).
+
+
+### g5.architecture.git_locations
+
+- agent
+- [Design](../../ontology-plan-git-abstraction.md).
+- Implemented for migrated operations · API paths remain selected by RepoStorage; host caches, guest manifest/member paths and CLI dev-container mirrors retain distinct owners. SCM resolves supplied bare/worktree/linked-worktree locations, refuses ancestor fallback for exact checkout, and preserves relative local remotes. Root/control-file, mirror/decoy, cache/restore and guest member/flush evidence is in the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation); complete deployed VM layout/performance coverage remains unexecuted.
+
+### g5.architecture.scm_provider_client
+
+- agent
+- [Design](../../ontology-plan-git-abstraction.md); the [login follow-up](../../chat_intent/design-git-abstraction/request.md#clarifications) narrowed AuthService's exception to OAuth login identity.
+- Implemented · `g5-scm::providers` owns App/installation, issues, candidate PAT/account discovery, stored-connection OAuth, PR lifecycle/read/checks and merge protocol. Separate login/report clients share private transport; API retains signing/cache, SQL binding, actor selection, verified-email linking, publication and outbox policy. Dead OAuth HTTP helpers and raw HTTP sharing are removed. Three full-scope independent discoveries, coherent repairs and a bounded independent repair recheck are recorded in the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). Live external provider execution remains unverified.
+
+### g5.architecture.scm_credential_scopes
+
+- agent
+- [Design validation](../../chat_intent/design-git-abstraction/request.md#validation-and-retry-clarification) avoids repository-scoped bootstrap and token-issuance cycles while retaining the landed credential owner.
+- Implemented · Candidate/account, App/installation, repository-capability, connection OAuth, login and report grants retain distinct authority. Repository credentials preserve initiating-user PR creation, installation-only automatic push and narrow read permissions; issuance never recurses into credential selection. Wrong-capability calls fail before HTTP. Real restricted-role API tests with mock providers cover login/connection separation, App/personal selection, import/origin admission and PR recovery; see the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). External permission/revocation matrices remain unexecuted. The credential owner retains its already validated typed provider instead of a second lookup; mismatched-origin and narrowed-token fixtures compile, with final DB rerun deferred to CI. The externally alleged panic was not reachable under the existing effective-provider check.
+
+
+### g5.architecture.github_transport
+
+- agent
+- [Login follow-up](../../chat_intent/design-git-abstraction/request.md#clarifications) and [design](../../ontology-plan-git-abstraction.md).
+- Implemented · One private bounded transport supplies standard headers, redirect refusal, no automatic replay, owned failure evidence and redaction to all live typed clients. Repository App API/web configuration, login OAuth endpoints and report API/web destinations stay independent. Selected origin/port/prefix and returned PR/report identity checks are tested through client and actual attached-repository owners; report assets use commit-pinned configured URLs. See the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). Live SaaS/Enterprise deployment and actual installation permission issuance remain unexecuted. SCM web/API configuration also composes a personal Enterprise integration without an App, with App configuration retaining precedence. The default/prefix config test passes; the corrected personal Enterprise PR/check fixture compiles and awaits CI runtime verification.
+
+
+### g5.architecture.git_guard
+
+- agent
+- [Design](../../ontology-plan-git-abstraction.md) and [login follow-up](../../chat_intent/design-git-abstraction/request.md#clarifications); extends the [physical dependency contracts](../../../backend/g5d/tests/architecture/source_contracts.rs).
+- Implemented as structural safeguards · `authoritative_workspace_reader_guard.rs` scans all backend packages including SCM siblings, restricts native construction to its runner, checks backend dependencies and exported native/libgit2/HTTP handles, and confines API content/ref operations to named owners. Thirty-two checks include new files/crates, aliases, associated-call and whitespace bypasses, typed private-helper controls and ordinary SQL commit. CI selects the integration binary as a required test; doctests alone are not hard-gating. Arbitrary macro/dataflow/type behavior or a new semantic duplicate inside its permitted owner is not proved. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
diff --git a/.g5/ontology/software_architecture/platform.g5.md b/.g5/ontology/software_architecture/platform.g5.md
index 32f75b19..05772dfd 100644
--- a/.g5/ontology/software_architecture/platform.g5.md
+++ b/.g5/ontology/software_architecture/platform.g5.md
@@ -88 +88 @@ Callers retain their domain authorization.
-- ~100% of inspected API separation · `auth_service.rs`, `auth/scm.rs`, routes `auth.rs`; end-to-end provider login, token refresh and every import consumer were not exercised in this pass.
+- Implemented for inspected separation · AuthService keeps login identity/email linking distinct from candidate/account/stored SCM grants, while typed SCM clients own protocol. Repository instance composition is independent of login. API auth/OAuth/PAT/import and both credential-resolver suites pass with local provider fixtures; see the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation). External browser/provider login and complete revocation matrices remain unexecuted.
@@ -118 +118 @@ Callers retain their domain authorization.
-- ~90% of inspected outbox boundary · `platform_issue_intake.rs`, `platform_issue_github.rs`, `maintenance/platform_issue_outbox.rs`; durable retry and captured payload exist, but external GitHub delivery was not executed.
+- Implemented for inspected delivery boundary · PlatformIssueIntakeService and the existing outbox keep admission, captured payload, signing/cache and recovery; PlatformIssueGithubClient delegates typed delivery to SCM with an independent destination. Nine database/provider-fixture recovery cases preserve one-create uncertainty recovery and commit-pinned screenshots. See the [Git abstraction review](../../ontology-review-git-abstraction.md#final-provider-migration--initial-independent-consolidation); live external delivery remains unexecuted.
diff --git a/.g5/ontology/software_architecture/runtime.g5.md b/.g5/ontology/software_architecture/runtime.g5.md
index 01ae69d2..0fa76eb1 100644
--- a/.g5/ontology/software_architecture/runtime.g5.md
+++ b/.g5/ontology/software_architecture/runtime.g5.md
@@ -369 +369 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
-- ~85% · host rematerializer/cache; KVM comparison and eviction unverified.
+- Scoped evidence · `g5d-host::workspace_vm` replaces retained roots before SCM restore; public retained-hostile-control and restore tests pass. Foundation cache locking/cancellation and exact materialization retain native behavior. Caches are preserved, not replaced; KVM performance comparison and eviction remain unverified. [Git review](../../ontology-review-git-abstraction.md).
@@ -387 +387 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
-- ~90% last whole-block baseline · dispatcher mutation/stream permits; full public interrupted capture and fork/PTY/preview race matrix remain unexecuted. The memory-specific detached-writer edge case is recorded in [memory persistence](../features/agents.g5.md#store-native-memory-in-the-launch-repository).
+- ~90% · dispatcher mutation/stream permits; API fetch body-drop cleanup and cancelled push awaiter retaining admission through Git completion have native/public-route regressions. Guest owned operations retain locks/leases through cleanup; missing-HEAD Update settles admission, dropped checkpoint cleans its index, and real noninterrupting fork plus root Linux success/failure handover pass. Full fork/PTY/preview race matrix remains unexecuted. The memory-specific detached-writer edge case remains recorded in [memory persistence](../features/agents.g5.md#store-native-memory-in-the-launch-repository); the SCM migration does not close that boundary.
diff --git a/.g5/ontology/software_architecture/workspace.g5.md b/.g5/ontology/software_architecture/workspace.g5.md
index 484c0646..65b4d784 100644
--- a/.g5/ontology/software_architecture/workspace.g5.md
+++ b/.g5/ontology/software_architecture/workspace.g5.md
@@ -124 +124 @@ execution evidence under the [event contract](runtime.g5.md#event-facts-survive-
-- ~75% · `backend/g5-api-core/src/services/workspace_service.rs`, `workspace_fork_service.rs`; legacy member/branch operations remain public and full rename/add/remove/delete/bootstrap conformance is unassessed.
+- ~75% · `backend/g5-api-core/src/services/workspace_service.rs`, `workspace_fork_service.rs`; local repository preparation uses typed Git operations without publishing its candidate; preparation and checked org bootstrap regressions pass. Legacy member/branch operations remain public and full rename/add/remove/delete/bootstrap conformance is unassessed.
@@ -148 +148,2 @@ execution evidence under the [event contract](runtime.g5.md#event-facts-survive-
-- ~75% · `backend/g5-api-core/src/services/workspace_reader.rs`; receipt selection/pinning can race pruning, and every document/tree/download caller has not been assessed.
+- ~75% · `backend/g5-api-core/src/services/workspace_reader.rs`; captured commit timestamps and wholly unborn readiness now use typed Git operations, with API tests for later branch changes, invalid object targets and Home bootstrap. Receipt selection/pinning can race pruning, and every document/tree/download caller has not been assessed. Syntax-aware ownership guards reject method/associated-call, alias and whitespace bypasses while admitting canonical private helpers. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
+
@@ -154 +155,2 @@ execution evidence under the [event contract](runtime.g5.md#event-facts-survive-
-- ~75% · `backend/g5-api-core/src/services/workspace_fork_service.rs`; receipt pinning can race pruning, provider authority can follow current owner instead of recorded connection, and bounded operator cleanup escalation is absent.
+- ~75% · `backend/g5-api-core/src/services/workspace_fork_service.rs`; receipt pinning can race pruning, repository-scoped credential-transition recovery remains unverified, and bounded operator cleanup escalation is absent. Source capture uses the shared bounded committed-cut reader. Prior successful merge/capture evidence is retained in the review; combined tests of main's no-retraction landing contract await CI; no new retry state is introduced. [Abstraction audit](../../ontology-review-git-abstraction.md#abstraction-audit--final-assessment).
+
```

**Relevant and governing ontology:**

The cases below are design conclusions, not executed implementation evidence. Each
owner must expose the allowed operation and reject the counterexample; source assumptions
are listed at the end for coding. Full IDs are used for unchanged owners; added IDs
below have the `g5.architecture.` prefix.

| Commitment | Obligation and owner | Allowed case / counterexample | Design conclusion |
|---|---|---|---|
| [git_execution](ontology/software_architecture/git_governance.g5.md#one-git-execution-layer), [g5.architecture.commands](ontology/software_architecture/boundaries.g5.md#keep-complete-operations-with-their-state-owner) | Policy owners compose typed operations; `g5-scm` hides backend handles, arguments and environment assembly. | Full-context diff through an adequate shared operation / exposing `git2::Repository` or `run_git(args)` to callers. | Stable types and complete operations are required before removing direct dependencies. |
| [g5.architecture.content_reader](ontology/software_architecture/workspace.g5.md#durable-repository-reads), [g5.architecture.projection_owner](ontology/software_architecture/git_governance.g5.md#convergent-projection-owner) | Authorized reader retains source selection and pinning; maintenance uses declared typed readers. | Render the selected OID / a document service opening the current checkout through `GitRepo`. | Consolidation must preserve the higher reader boundary; library ownership alone is insufficient. |
| [git_ref_updates](ontology/software_architecture/git_governance.g5.md#keep-ref-effects-behind-typed-git-operations), [g5.architecture.git_owner](ontology/software_architecture/git_governance.g5.md#git-policy-owns-authoritative-ref-effects) | Git policy authorizes scope; native and libgit2 implementations enforce operation-specific preconditions and report effects. | Accepted mapped-branch update with exact old OID / treating a runner invocation as authorization to update arbitrary refs. | Native ref effects are covered without forcing them through libgit2. |
| [g5.git.network.cas](ontology/software_architecture/durability.g5.md#update-mapped-branches-through-exact-old-oid-cas), [g5.git.network.ref-scope](ontology/software_architecture/durability.g5.md#restrict-network-ref-effects), [g5.git.receipt.write-admission](ontology/software_architecture/durability.g5.md#admit-flush-writes-only-for-their-unfinished-receipt), [g5.governance.publish.proof](ontology/features/governance.g5.md#prove-initial-external-publication) | Existing Git/flush/publication owners retain separate grants and preconditions. | Identical immutable flush retry; first external publication create-if-absent / absent mapped ref treated as create permission or equal OID treated as publication proof. | Operation-specific rules govern; no universal unchecked ref setter. |
| [git_native_runner](ontology/software_architecture/git_governance.g5.md#run-native-git-through-one-runner), [g5.runtime.materialization](ontology/software_architecture/runtime.g5.md#host-git-never-trusts-retained-guest-control-files), [g5.runtime.bundle](ontology/software_architecture/runtime.g5.md#the-platform-supplies-its-complete-runtime) | Runner owns execution/cleanup; runtime supplies the trusted context and binary. | Bundled guest Git and host-owned materialization / trusting guest-retained hooks/config or abandoning a child on cancellation. | Preserve native mechanisms and verify native-only dependency closure. |
| [g5.runtime.workspace-cli](ontology/software_architecture/runtime.g5.md#make-the-workspace-cli-available-to-people-and-agents), [g5.runtime.workspace-cli.context](ontology/software_architecture/runtime.g5.md#supply-the-current-workspace-context-to-the-cli), [g5.runtime.drain](ontology/software_architecture/runtime.g5.md#streams-and-mutations-have-different-drain-lifetimes) | Runtime owns CLI environment, operation admission and drain policy; runner consumes them. | Same `g5d` launcher environment and bounded cancellation / reconstructing PATH, bypassing sandbox policy or treating streams as mutations. | Runtime inputs flow down to `g5-scm`; it does not import orchestration. |
| [g5.git.durability.working-state](ontology/software_architecture/durability.g5.md#preserve-the-users-git-working-state-during-flush), [g5.git.flush.content](ontology/software_architecture/durability.g5.md#capture-content-without-collapsing-its-ancestry), [g5.git.exclusions](ontology/software_architecture/durability.g5.md#keep-platform-artifacts-outside-native-git) | Flush/member setup retain temporary-index isolation, ancestry, identity and exclusions. | Per-operation temporary index / changing real HEAD/index or committing runtime credentials. | Runner API must preserve these inputs without public environment overrides. |
| [g5.git.checkpoint.selected](ontology/software_architecture/durability.g5.md#commit-and-push-only-selected-file-changes), [g5.git.checkpoint.isolation](ontology/software_architecture/durability.g5.md#preserve-unrelated-git-state-during-a-checkpoint), [g5.git.checkpoint.retry](ontology/software_architecture/durability.g5.md#retry-a-failed-selected-file-publication) | Existing checkpoint owner accepts expected file versions and preserves unrelated staging. | Retry publication of the same selected version / silently committing a changed version or unrelated staged work. | Newly landed checkpoint behavior is part of migration coverage. |
| [git_outcomes](ontology/software_architecture/git_governance.g5.md#report-what-a-git-or-provider-operation-established), [git_retry](ontology/software_architecture/git_governance.g5.md#leave-new-retry-decisions-to-the-agent-or-human), [g5.architecture.recovery](ontology/concepts/system_design_principles.g5.md#make-failures-and-retries-explicit) | Execution reports effect evidence; agent/human chooses retry through the existing owner. | Lost PR reply reported as unknown with a view/check instruction / blind replay, a new retry table or a timeout reported as rollback. | No new retry persistence or automatic retries; existing workflow recovery remains outside this layer. |
| [g5.architecture.workspace_registry](ontology/software_architecture/workspace.g5.md#workspace-identity-and-membership), [g5.data.git-authority](ontology/concepts/data_model.g5.md#git-owns-repository-content) | Workspace owners select members/default branches; repository preparation configures storage without publishing authority. | Unattached initial candidate and wholly unborn repository / treating a missing branch in populated storage as empty or moving a branch during preparation. | Preserve bootstrap admission, immutable timestamp selection and checked publication while removing API core backend access. |
| [git_credentials](ontology/software_architecture/git_governance.g5.md#inject-git-credentials-in-one-place), [g5.security.credentials.publication](ontology/concepts/security.g5.md#keep-provider-publication-credentials-server-side), [g5.security.credentials.platform-token](ontology/concepts/security.g5.md#expose-the-incarnation-token-under-the-mvp-boundary), [g5.security.credentials.redaction](ontology/concepts/security.g5.md#redact-secrets-from-public-output) | Credential owner selects; `g5-scm` injects per invocation and redacts. | Guest incarnation token; API provider grant / token in argv, URL, persistent config or guest provider credentials. | Sharing the injector grants no new credential authority. |
| [git_locations](ontology/software_architecture/git_governance.g5.md#resolve-repository-locations-from-their-owners) | Storage/materialization/guest/dev-container owners provide distinct locations. | Adapter resolves Git directory from a supplied worktree / treating a dev mirror as an API storage root. | Share types and equivalent mechanics, not unrelated layouts. |
| [scm_provider_client](ontology/software_architecture/git_governance.g5.md#one-client-per-repository-provider), [scm_credential_scopes](ontology/software_architecture/git_governance.g5.md#match-provider-credentials-to-the-operation-scope) | Typed clients own protocol; identity/App/repository owners supply correctly scoped grants. | Validate an unbound candidate PAT or list account organizations / inventing a repository or recursing through repository selection to mint an installation token. | Separate pre-repository and repository scopes; provider dispatch only at composition. |
| [g5.architecture.pr_flow](ontology/software_architecture/git_governance.g5.md#one-create-or-update-pr-flow), [g5.governance.publish.github-author](ontology/features/governance.g5.md#attribute-github-pr-creation-to-the-initiating-user), [g5.governance.pr.follow-push](ontology/features/governance.g5.md#follow-successful-source-pushes-automatically) | Shared PR owner retains user authorship, accepted-push continuation and manual Update PR recovery. | Confirmed push signals current-head refresh / bot fallback, lost refresh signal or new pending-state columns. | Preserve landed behavior and existing outcome reporting. |
| [g5.architecture.pr_checks](ontology/software_architecture/git_governance.g5.md#one-provider-check-read-operation), [g5.governance.pr.checks](ontology/features/governance.g5.md#inspect-provider-checks-from-a-workspace-agent), [g5.governance.pr.checks.revision](ontology/features/governance.g5.md#bind-check-observations-to-the-requested-revision) | PR read owner binds checks to authorized repository/revision; client owns pagination/normalization. | Bounded check reads for the requested head / newer-head evidence presented as requested-head success or a read triggering publication. | Include the landed checks client; polling remains the existing CLI read workflow. |
| [github_transport](ontology/software_architecture/git_governance.g5.md#share-one-github-http-transport), [g5.architecture.identity_owner](ontology/software_architecture/platform.g5.md#login-identity-and-repository-connection), [g5.architecture.report_owner](ontology/software_architecture/platform.g5.md#durable-platform-bug-report-intake), [g5.security.login](ontology/concepts/security.g5.md#keep-platform-login-distinct-from-repository-integration) | Shared transport supplies mechanics; login, provider and report clients retain independent host/grant/operation policy. | Enterprise repo calls and separate platform report destination / credential forwarding across redirect or account lookup using a repository bot token. | Keep host isolation, endpoint formats and typed error meanings; expose rate limits without retrying. |
| [git_guard](ontology/software_architecture/git_governance.g5.md#guard-against-new-git-bypasses), [g5.quality.evidence](ontology/concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence) | Ownership guards preserve existing reader/integration checks and reject configured-URL and raw-handle bypasses. | Rename a private backend helper / new raw HTTP through a configured URL or a removed search target making a guard pass. | Negative and harmless-refactor cases required; static checks do not prove runtime behavior. |
| [g5.scope.capacity-recovery.gc](ontology/concepts/operations.g5.md#serialize-repository-garbage-collection), [git_native_runner](ontology/software_architecture/git_governance.g5.md#run-native-git-through-one-runner) | Existing API maintenance owns scheduling/thresholds and advisory locking; SCM executes and owns child cleanup. Later retained-source/reader serialization is separate. | Shutdown reaps active GC before sweep unlock / moving SQL policy into SCM or implying new pruning safety. | Preserve current GC semantics and explicit future scheduling limits. |
| [g5.design.information-hiding](ontology/concepts/system_design_principles.g5.md#keep-private-representations-private), [g5.architecture.compatibility](ontology/software_architecture/boundaries.g5.md#compatibility-preserves-callers), [g5.design.abstraction](ontology/concepts/system_design_principles.g5.md#create-abstractions-around-stable-meaning), [g5.design.minimal-api](ontology/concepts/system_design_principles.g5.md#expose-minimal-sufficient-apis), [g5.architecture.commands](ontology/software_architecture/boundaries.g5.md#keep-complete-operations-with-their-state-owner), [g5.architecture.dependencies](ontology/software_architecture/boundaries.g5.md#direct-dependencies-toward-shared-capabilities) | Shared mechanisms depend downward; callers retain minimal complete operations. | Runtime passes resolved environment to runner / `g5-scm` importing g5d orchestration, SQL binding policy inside transport, or `g5-semantic` depending on Git merely for author email. | Native features, transport placement and reused operations require coding-stage inspection. |

## Evidence and scope

The [supplied Git audit](https://claude.ai/artifact/G1VgAmoseT5X5yagrYrthQ) evaluates the
original review at `05dddc47` with a later drift check. Its static findings qualify the
baseline; they have not been independently rechecked against this rebased implementation.

| Audit correction | Consequence for this design |
|---|---|
| Nine manifests mention `git2`; four crates use it in production; five cited files use trivial value helpers. | Separate real repository access, missing API support and dependency cleanup. |
| Native guest Git, receive-pack and GC have legitimate owners; `g5-scm` is already shared with host crates. | Consolidate execution mechanics while retaining owners and native protocols. |
| Four credential injections and one production binary resolver; the cited OAuth fetchers are dead. | Inventory live paths; distinguish header clearing and test helpers; remove confirmed dead code. |
| Dev-container mirror layout differs from API storage; `repo_git_dir` selects a Git directory from a storage path; the hard-coded URL is largely a replaced placeholder. | Preserve location kinds; do not merge layouts on the strength of string matches. |
| Full-context diff support is missing; projection ref reads bypass shared readers; partial guards exist. | Complete the API, repair the actual read bypass and extend existing enforcement. |
| Resolution refs are per-run; `http()` is documented sharing. | Specify per-run preconditions; typed provider operations are an intentional tightening of today's interface. |

This is a platform-internal design for a single execution boundary, sufficient typed
operations and shared provider protocol. It preserves authorization, user-visible Git
semantics and existing storage. The only new failure contract is explicit effect/error
reporting with caller-decided retry; no new retry persistence or automatic replay is added.

Out of scope: interactive user/agent Git; changing push-admission SQL ownership;
replacing existing durability/lifecycle recovery; the independent frontend
`useBranchSyncRuns` duplication; implementing this design during the planning turn.

## Design

### Preserve owners above the execution boundary

Application content consumers use the existing authorized workspace reader, retaining
`ResolvedConnectedBindingView`, `CommittedRepoReadView`, pinned OIDs and source-vector
consistency. Its implementation uses `g5-scm`. Maintenance uses the projection owner's
typed readers. Git policy, publication, checkpoint and materialization owners compose
complete lower-level operations; HTTP/CLI entry adapters do not reconstruct those flows.

`g5-scm` exports backend-independent OID/ref/location inputs and typed outcomes.
Its capabilities cover object/ref reads, buffer/tree diffs (including full-file context),
checked ref changes, native fetch/push and pack streams, materialization, GC and member
setup. Existing workflow operations such as selected-file checkpoint remain intact.
Raw backend repositories, argument lists, unrestricted environment maps and HTTP clients
are private. Named methods that merely relocate those escape hatches are insufficient.

Libgit2 and native Git remain private implementations behind separate Cargo features;
common types and outcomes require neither backend. Host/guest can enable native execution
without libgit2. Do not replace native protocol serving with libgit2 or move domain policy
into the runner. `g5-semantic` receives author email as input rather than acquiring a Git
dependency for that value.

### Cover native and libgit2 ref effects

Each ref-changing operation names its repository, ref set, preconditions, transaction
scope and effect evidence. These semantics apply even when the native program performs
the write internally. A general permission to run Git cannot replace checked ref scope.

| Operation kind | Required behavior |
|---|---|
| Existing mapped branch network update | Current policy admission, exact old-OID CAS and existing denied-ref rules; absent does not mean permission to create. |
| Protected import, external publication, immutable flush refs | Retain the respective import lease, create-if-absent/publication proof and receipt-bound immutability rules. |
| Per-run conflict-resolution pair | Create only when absent, accept an identical pair as a no-op, reject collision; intentional replacement names both expected values in one checked transaction. |
| Native receive-pack | Preserve the accepted ref set and the protocol's existing atomic or per-ref behavior. Report confirmed, refused and unknown outcomes separately; signal only confirmed accepted changes to the policy/publication side. |
| Local fetch/cache/checkout | Declare local tracking/HEAD effects under the materialization owner; do not import mapped-branch write grants or merge newer internal heads during restore. |

Coupled updates remain coupled. Do not promise cross-repository or Git/provider atomicity,
or infer rollback from a nonzero exit. A precondition refusal leaves its transaction's
refs unchanged; independently admitted transactions may still succeed. Failures after
effects follow the outcome contract below. Existing locks, admission and
transaction phases must be inspected before reuse rather than inferred from function names.

### Native execution and context

One runner owns command construction, binary selection, environment, stream/child lifetime
and redacted errors. It accepts the caller's deadline/cancellation and retains cleanup
ownership until child termination is established; cancelling transport does not undo
remote effects. Existing runtime admission and stream/mutation drain policy remains above it.

Context is selected at trusted composition boundaries. API execution uses its configured
resolver; platform host/guest execution uses the bundled Git. A platform CLI running in
the guest consumes that guest context. Any supported external CLI context retains its
existing approved lookup policy, to be checked during coding. Contexts define ambient
config, allowed inherited `GIT_*` variables, hooks and `safe.directory` behavior; host
materialization ignores guest-retained control files.

The runtime owner supplies PATH/loaders from the landed workspace-CLI setup as input;
`g5-scm` must not depend upward on `g5d_runtime_core`. Callers select named operations,
not individual environment flags. Member configuration and author identity are explicit
operations, never global/system config writes. Temporary-index capture and selected-file
checkpoint preserve real index/HEAD, unrelated staging, expected file versions and ancestry.

### Errors and caller-decided retry

Operations distinguish **refused before effects**, **completed**, **partially completed**
and **outcome unknown**. Error evidence includes the operation and authorized target,
known completed effects, the failed stage/cause, unknown effects and useful next action.
Include sanitized provider request IDs, ref OIDs, failure categories and retry-after
hints when available; do not expose secrets or invent evidence the provider did not return.

Examples:

- A lease mismatch identifies the expected/observed ref when available and asks the caller
  to inspect current state; it does not replace the lease and push again.
- If Git push succeeded but provider publication failed, preserve the successful push and
  report the remaining provider failure through the existing PR outcome/UI/CLI surface.
  The agent/human may correct the cause and invoke Update PR or `g5d pr update`.
- If a PR-creation response is lost, report an unknown result and direct the caller to
  inspect existing PRs before resubmitting; do not repeat creation automatically.
- A rate limit returns its retry-after hint. The abstraction does not sleep and retry.
- A selected-file publication failure retains the existing same-version retry contract;
  a later edit requires a newly accepted version, not a silent broadened checkpoint.

**No new tracking tables, retry columns, queues, journals or retry workers.** Reuse
existing domain outcome reporting and read/view operations. The new layer performs no
automatic operation replay; agent/human retry re-enters the existing owner with current
authority and preconditions. Existing independently governed flush/lifecycle/merge/outbox
recovery and PR-check polling retain their owners and do not become a generic retry
service inside `g5-scm` or `GithubHttp`. Bounded pagination is part of a read operation,
not permission to replay a failed write. No exactly-once promise is introduced.

### Credentials and provider operations

Only `g5-scm` injects a selected Git credential into native invocation or libgit2 callbacks.
Injection is per invocation, bound to the intended destination, absent from argv,
persistent config and remote URLs, and redacted before errors return. Inspect the live
injection and clearing sites before consolidation. Use supported per-invocation Git
config environment entries or a private credential-helper channel; verify runtime version
and preserve stdin for pack/protocol data. Guest execution holds only incarnation credentials;
provider publication tokens remain server-side.

One implementation per repository provider supplies capability-specific typed operations
behind `ScmProvider`: PR lifecycle/read/checks, merges, issues/comments, connection/account
discovery and App operations. Unsupported capabilities have typed outcomes; consumers do
not branch on provider-name strings. Provider construction/selection at composition is
legitimate. Keep database-backed installation binding and account policy in their existing
owners, even if the protocol adapter lives in the same crate.

| Credential scope | Selecting owner and provider input |
|---|---|
| Candidate connection | AuthService supplies the candidate token for validation before any connection/repository record is required. |
| Account discovery | Account/SCM-connection owner supplies the user's connection grant for organizations/groups and repository discovery. |
| App/installation | App owner supplies the App or installation grant for discovery/token issuance; issuance is a primitive used by credential selection and never calls back into repository selection. |
| Repository operation | `repository_credentials` supplies the operation-specific grant: initiating-user authorization for PR creation, existing installation-only policy for AutoPush, least-privilege read grants for checks. |

Types preserve scope, host and operation, rather than treating a bearer string as authority
for any request. Clients execute the supplied grant without selecting another actor,
fallback or token lookup. Credential lifetime/cache policy stays in the selecting owner.
No fictitious repository is needed for account/bootstrap operations.

Remove the raw `http()` sharing accessor after its consumers have sufficient typed
operations. Inventory the newly landed PR lifecycle/read/check clients, live AuthService
calls and the audit's reported dead OAuth functions. Delete confirmed dead functions;
do not migrate them into the new transport as purported live functionality.

### Shared GitHub transport and repository locations

`GithubHttp` serves only typed provider, OAuth-login and platform-report clients. It owns
URL construction/parsing, standard headers, transport-level failures and rate-limit
evidence. Those clients retain endpoint-specific formats and provider error meaning.
No credentials are stored or selected by the transport; each request receives the
appropriate grant. No redirects forward credentials.

Each owner retains its own web/API base: repository integration may use Enterprise,
login follows its OAuth configuration and platform reports follow their independent
destination. Preserve valid endpoint overrides and content types. Validate PR/issue/check
URLs and pagination destinations against the appropriate owner before credentialed use.
Test live login endpoints; dead URL literals do not establish live host-routing defects.
Place the transport below all three clients without importing routes, SQL binding policy
or runtime orchestration. Exact module placement is a coding-stage dependency check.

For locations, keep `RepoStorage` authoritative for API roots/URLs. Host caches, guest
member checkouts and dev-container mirrors retain their own layouts. Share typed location
values and equivalent helpers; do not impose one root scheme on all three. Git-directory
selection from a supplied bare/worktree location belongs inside the execution adapter.
Only add a storage URL operation where a live consumer needs it; clean up a placeholder
or test-only path according to its actual owner.

### Enforce the intended boundaries

Extend the existing authorized-reader, remote-integration and crate-dependency guards;
do not discard them when adding a Git-spawn guard. Use visibility and public API checks
alongside calibrated source contracts. Cover:

1. Production libgit2 imports/dependencies outside `g5-scm`, including aliased/target-specific
   declarations, and public backend handles or generic argument/environment passthroughs.
2. Native Git spawns outside the runner, including configured program paths. Preserve
   legitimate user/agent shell execution; fixtures distinguish it from platform helpers.
3. Application content reads bypassing the authorized reader even through `g5-scm`, and
   projection ref reads bypassing the maintenance owner's typed path.
4. Raw provider HTTP outside permitted typed clients/transport, including a configured URL
   without any `github.com` literal. Check module dependencies and request construction;
   permit unrelated HTTP only under its separately named owner.
5. Provider-name dispatch outside composition, and raw HTTP/client escape hatches returned
   to consumers. Host-literal checks are supplemental, not the enforcement criterion.

Migration allowlists name the specific existing operation, reason and destination owner;
whole-file exemptions must not allow new bypasses. Each check has a violating case and a
harmless refactor, plus a missing-target case where applicable so removed code cannot
make an intended check vacuously pass. Repair the audit-reported stale push-order check
against the real operation after source inspection. Guards are structural safeguards,
not proof that arbitrary dynamic code can never bypass a boundary.

### Implementation order

Each change preserves its existing native/libgit2 mechanism unless separate evidence
justifies changing it. Native Git and libgit2 are legitimate implementations of different
operations; code motion alone establishes no behavioral equivalence.

1. Inspect live call paths on this rebased base. Classify each as an existing owner,
   actual bypass, missing shared operation, value helper, test helper or dead code.
   Resolve the source assumptions below and amend this plan if intent changes.
2. Extend existing guards as a ratchet with precise migration exceptions. Define common
   types, complete operation inputs/outcomes and authorized-reader preservation first.
3. Add native runner contexts/cleanup and credential injection. Migrate host/guest/CLI
   calls without linking libgit2 or merging distinct repository layouts.
4. Move API protocol execution and other native helpers, preserving push admission,
   confirmed-ref refresh signals, selected-file checkpoint and flush behavior.
5. Complete shared diff/ref/read APIs, migrate direct libgit2 consumers, and remove
   unused production dependencies. Preserve the authorized content-reader boundary.
6. Add scoped provider inputs and shared transport. Consolidate live clients, including
   landed PR/check functionality, remove confirmed dead code and retire raw HTTP sharing.
7. Empty the production migration exceptions and verify the acceptance cases. No schema
   migration, retry worker or additional tracking mechanism is part of these steps.

## Initial implementation slice

The following implementation sections retain incremental history. Their pending-work
notes are superseded by subsequent increments and the final completion assessment.

The approved design is preserved in commit `3f7d2874`. The initial implementation
replaces the live direct libgit2 bypasses in resolution-input pinning, projection ref discovery and semantic
full-context diffs, and extends the existing reader guard for those operations.
Native execution, provider consolidation and the remaining backend consumers stay
explicit migration work; this slice does not claim the whole boundary is closed.

Source inspection confirms that `git2::Transaction` locks refs together but commits
them one by one without rollback on I/O failure. Resolution pins therefore check
both refs under lock before staging changes, report a pre-write refusal separately
from an uncertain commit failure, and direct the caller to inspect both refs before
retrying. This applies the existing outcomes contract; it does not add automatic
replay or tracking storage. The provider-resolution policy owner reuses a run ID after
an enqueue/SQL rollback:
its existing row lock and absent `resolution_task_id` establish abandoned preparation.
It captures the current optional pair and requests replacement with both expected
values; `g5-scm` checks them under both ref locks before writing. Committed tasks keep
their pins, while fresh workspace-resolution runs use create-or-confirm. This preserves
existing manual retry, including a partial pair, without adding tracking state.
The safe API error envelope retains the effect category, failure stage, repository/ref
identity and corrective action; its human-visible message states whether refs may have
changed, while raw backend diagnostics remain scrubbed internal evidence.
Existing replay-command construction remains in the agent-task policy owner; the
Git operation returns a typed related/unrelated history result, not command arguments.

Projection reads must still distinguish a missing ref from a missing mirror, dangling
symbolic ref or invalid commit. Reusing `read_branch_oid` would lose that distinction
because it intentionally does not inspect the target object. Add the narrow peeled
commit readers needed by the maintenance owner. Full-context buffer diffs preserve
the ordinary helper's default context and the caller's existing output budget.

## Native execution migration

The next increment introduces the backend-independent native runner and migrates
CLI smart-HTTP materialization and workspace push. It preserves remote identity,
fetch/refspec scope, fast-forward admission, explicit old-head leases and per-member
partial publication reporting. The CLI retains manifest and retry policy; the runner
accepts complete clone/fetch/push operations, never caller-supplied Git arguments or
environment maps. Runtime/host/API consumers and the existing libgit2-backed lease
helper remain named migration work until their distinct environments are moved.

Credentialed native operations validate the actual Git destination before injecting
credentials. Invocation-only, URL-scoped headers replace CLI argv headers; redirects
cannot forward them. Existing guest runtime credential isolation is not replaced by
this CLI context. Errors retain failure evidence and indicate possible effects after
execution begins; this layer adds no retry, persistence or automatic replay.

Execution deadlines cover the child and output collection. A private supervisor owns
cleanup even if its awaiting future is dropped; cancellation stops the child process
group where supported, reaps the child and closes/drains its owned streams. Output
retention is bounded. Synchronous callers use the same runner under a private runtime.
Native-only feature checks prove the shared crate can omit libgit2; complete host and
guest dependency closure remains unproven while their other GitRepo uses remain.

The CLI context clears inherited repository-selection and per-command Git configuration
channels before applying operation-owned controls; it retains runtime PATH/loaders and
its declared isolated config. The guest CLI context resolves only the runtime-owned
member relay mapping, keeps canonical identity separate, and does not inject a platform
token into repository Git. Ordinary CLI rewrites remain refused. CLI signal ownership
cancels its shared native execution context and waits for cleanup before exit; it does
not change the host/dispatcher lifecycle owners. Porcelain remote failure remains an
unknown write even when it uses the same flag as a definite rejection. Confirmed
ref evidence survives interrupted output collection. Fresh guest clones resolve the
canonical member in the runtime-owned mapping independently of ambient cwd or the
existence of a destination checkout; the invocation preserves the canonical origin.

## Existing native lease migration

The reviewed typed-operation/native CLI checkpoint is committed as `156ca53e`.
The next increment removes the older `GitRepo` native lease subprocess implementation
and routes its reviewed rewrite and exact-head compensation-delete operations through
the same supervisor. Existing policy helpers keep choosing when rewrite or compensation
is authorized; ordinary fast-forward publication remains unchanged. Publication callers retain the absent-head lease input. Git may report an already-equal
destination as unchanged, so this alone is not first-publication proof; that inherited
gap remains recorded. Deletion explicitly supplies its expected old head.

Provider grants retain exact trusted HTTPS-origin restrictions and redirect refusal.
The legacy platform-token GitHub SSH identity normalization remains a transport choice
without changing the stored remote; its resulting HTTPS push destination must also pass
rewrite validation. Relative local remotes retain repository-relative Git semantics.
SSH keys keep their supported authentication path;
private temporary key/askpass resources belong to the invocation and survive until child
cleanup, including cancellation. Credentials and output stay redacted and outside argv.
The shared native API exposes a typed leased deletion, never an arbitrary refspec or
command escape hatch. The legacy adapter keeps useful rejection guidance and receives
confirmed, refused or unknown effect evidence. Native-only consumers still omit libgit2;
`GitRepo` retains native execution as an implementation dependency for its existing lease
operations. No new recovery state or automatic retry is introduced.

## API core backend closure

This increment removes API core's remaining production libgit2 use. The authorized
reader and workspace membership owner retain source/branch selection and use existing
immutable commit-time/ref operations. Bootstrap readiness must still distinguish a
present commit, a wholly unborn repository with no refs, and an unavailable selected
branch; a dangling or non-commit target must not become an empty-repository success.
Missing storage remains the caller's distinct case. Repository initialization stays a
preparation step: its typed Git operation may open/create storage and configure symbolic
HEAD, but does not publish a branch or authorize the candidate commit. Existing policy
owners still validate the requested branch before storage effects and publish through
their checked flow.

Git blob hashing, parsed-OID zero checks and ref syntax belong behind the shared API.
Keep artifact storage/limits, observation cache policy and workspace naming policy in
API core; retain their existing error categories and supported value semantics. Move
`git2` to API core's test dependencies only after all live consumers are migrated.
Extend the existing structural guard across production API core, including missing
expected operations, without admitting new reader bypasses or rejecting test fixtures.
No new storage, retry behavior or runtime lifecycle effects are introduced.

Governing commitments are `g5.architecture.git_execution`, `git_ref_updates`,
`git_locations`, `git_guard`, the [authorized reader](ontology/software_architecture/workspace.g5.md#durable-repository-reads),
[workspace registry](ontology/software_architecture/workspace.g5.md#workspace-identity-and-membership),
and [Git content authority](ontology/concepts/data_model.g5.md#git-owns-repository-content).
The existing contracts cover this migration; no governing meaning change is needed.
Source inspection found an existing `commit_committed_at` operation, which must be
reused rather than duplicated. Broader workspace-ops/raw-handle, native host/API and
provider HTTP closure remains separate migration work.

## Workspace operations backend closure

Move unattached exact-tree and initial-commit construction into typed `g5-scm`
operations, keeping their complete validation, path/mode behavior, immutable parent,
no-op result and private-index semantics. Preparation writes objects only; the existing
API policy owners still protect and publish candidates. Workspace-ops retains its
public preparation entry points through re-exports and keeps checkpoint, merge and
multi-repository composition. No caller receives an index, repository handle or backend
callback. Clone, init and current-branch reads also become typed shared operations,
preserving selected/default branches, clone depth and existing transport variants.

Source inventory found no production caller for the old isolated-worktree write APIs
or repository-local safe-directory setter. Remove those unused operations instead of
moving them into the shared public API. This also retires the unused check-then-force
branch write; it does not replace the checked landing owner. The live global safe-directory
helper and its CLI callers remain part of the later native-context migration, not a
claim of global-config conformance. Existing runtime and guest-interactive behavior
must be resolved together before changing that path.

After migrating every live caller, remove the public raw repository constructor/lock
and make libgit2 credential callback assembly private to SCM. Tests open fixture paths
through the public API. Move workspace-ops' libgit2 dependency to dev-only and extend
source/dependency and public-interface guards. Existing exact preparation tests must
still reject invalid batches before object writes and preserve unrelated refs, index,
checkout bytes, symlink/executable modes and literal pathspec-like names. Exercise the
API's candidate/landing and review consumers and CLI materialization callers. No new
retry state, replay, provider grant or sibling-object confidentiality mechanism is added.

Review exposed an inherited initial-file batch gap: ancestor collisions could drop
requested content and malformed later paths could leave objects before refusal. The
shared operation must validate the complete normalized batch before object writes,
retaining the initial `.g5` exclusion and accepted trimming. Public-interface checks
must reject backend-bearing signatures and re-exports regardless of the accessor name,
while allowing private backend helpers and public owned wrappers.

This applies the existing `git_execution`, `git_ref_updates`, `git_credentials`,
`git_locations`, `git_guard` and Git-authority commitments; no governing meaning change
is needed. Reused preparation operations were inspected before migration. Broader native
and provider consolidation remains open.

## API garbage-collection execution

Migrate the existing API/indexer GC subprocess into a complete shared native maintenance
operation. API maintenance retains repository enumeration, advisory locking, sequential
scheduling, thresholds, logging and per-repository error isolation. SCM resolves bare or
worktree storage, counts loose objects, runs the existing `git gc` behavior when admitted
by the supplied threshold, and reports skipped or completed counts. No new pruning,
retention or active-reader serialization policy is introduced; the later
[GC scheduling contract](ontology/concepts/operations.g5.md#serialize-repository-garbage-collection)
and its current limits remain separate.

An API runner context consumes the API-selected executable, resolving a bare name against
the existing fixed API PATH once per handle, and clears the inherited environment while
retaining that fixed PATH and existing repository/system Git configuration semantics.
It does not write global/system settings. Share process/stream supervision with the CLI
runner without changing CLI environment behavior. Invalid count output is a read failure;
failure after a confirmed GC records that completion even if the final count is unavailable.
The API retains the completed sweep count and the failed-inspection warning; unknown or
refused attempts never count as confirmed collection.
Shutdown cancellation reaches the active invocation, and the sweep waits for cleanup
before releasing its advisory lock. Existing sweep scheduling is retained; no automatic
operation replay or retry storage is added.

Governing commitments are `git_execution`, `git_native_runner`, `git_locations`,
`git_outcomes`, `git_retry` and `git_guard`, with API maintenance retaining its existing
policy ownership. Verify real packing and preserved refs/index/checkout, bare and missing
storage, threshold refusal, malformed output, completion followed by failed inspection,
deadline/cancellation cleanup, inherited-environment isolation, and DB-backed lock/error
isolation. Extend the existing structural guard to reject native construction in this
migrated owner while permitting test fixtures and helper renames. This is an implementation
of the approved contracts, not a governing meaning amendment.

## API Git proof observations

Move ref-tip and symbolic-ref inspection, object-type verification and ancestry reads
used by receive-pack admission and flush completion into typed native SCM operations.
The API retains authorization, admission locks, exact old-OID comparisons, immutable
receipt rules and accepted-push notifications. These observations do not publish refs
or authorize content reads outside the existing owners. Protocol process execution
and its streaming/admission lifetime remain a subsequent migration.

The API supplies its resolved binary and a 30-second deadline per native invocation.
SCM validates exact ref names and repository-relative full OIDs, explicitly anchors
bare/worktree/linked-worktree Git directories without ancestor discovery, and clears
inherited controls and owns child cleanup. Ref absence and missing/non-commit objects
are ordinary proof observations; repository failures, malformed/truncated output,
deadlines and cancellation are explicit errors rather than proof of absence. Object
inspection combines one batch-check record with diagnosed storage errors to distinguish
missing objects from execution failure while retaining harmless platform warnings; its private stdin writer shares supervision with output draining.
SHA-1 and SHA-256 full OIDs are accepted for these reads; abbreviations are refused
against the selected repository format. Wrong-width object inputs remain permanent
flush proof conflicts.
Existing flush proof codes remain; operational failures retain `FLUSH_UNAVAILABLE`
with sanitized operation diagnostics. Receive-pack consumes confirmed per-ref acceptance
without an optional read; needed fallback failures preserve successful siblings and their
queue/events/reviewer continuation before returning partial/unknown effect evidence.
No retry mechanism or persistence is added.

This implements `git_execution`, `git_native_runner`, `git_outcomes`, `git_retry`,
`git_locations` and `git_guard`, under `g5.git.receipt`, receipt write-admission and
network CAS/ref-scope. Verify bare/worktree reads, symbolic and absent refs, all object
types and missing objects, ancestry, malformed output, deadline/cancellation cleanup,
API proof refusal without receipt advancement, and existing push/flush regressions.
Extend guards over the fully migrated flush owner and the declared native observation
operations; do not claim that the remaining protocol owner is fully migrated.

## API smart-HTTP protocol execution

Move advertisement, upload-pack and receive-pack execution into typed native SCM
operations. API authentication, storage selection, push admission, immutable replay
and accepted-ref publication remain the existing owners. Receive execution declares
the admitted old/new/ref vector; SCM verifies that the actual request commands match
it before spawning, retains the client's atomic/per-ref mode, and returns per-ref
protocol evidence alongside any execution failure. A failed process/deadline does not
remove already captured acceptance evidence or imply rollback.

The API supplies a five-minute execution budget per protocol invocation. Fetch streaming
excludes time blocked delivering bytes to the HTTP reader from that budget; waiting for
Git to produce data and finish still consumes it. A reader that stays connected but
accepts nothing for a whole budget ends the execution, so a silent client cannot hold
Git indefinitely; a slow reader that keeps accepting bytes is unaffected. Reader
disconnect still cancels and reaps the child. This external-review clarification repairs slow-reader truncation;
the original wall-clock wording is retained in the pre-review head `436b186d`.
SCM uses the configured
API binary, fixed API PATH, explicit repository/worktree identity, existing TLS CA inputs
and non-fast-forward/updateInstead receive policy. Diagnostic output stays bounded;
protocol bytes are preserved without the diagnostic truncation limit. Fetch responses
stream through a bounded buffer, concurrently with input delivery and stderr draining.
Dropping the response cancels and reaps its process; EOF is successful only after process
completion, so a failed upload-pack cannot become a silently successful body.

An admitted push is owned by a task that retains its SQL admission until native execution
and cleanup finish, even if the HTTP awaiter disappears. It then performs the existing
confirmed-ref continuation after releasing admission. Release/publication failures retain
known Git effects and inspection/Update PR guidance; an independent enqueue failure does
not suppress confirmed branch hints or other accepted continuations. An uncertain admission
release skips independent DB work while retaining in-memory hints. The task has the native operation
deadline; it adds no persistent work, retry or replay. This preserves the mutation/stream
lifetime distinction and fixes the current child-outliving-admission gap. Process/runtime
crashes remain uncertain outcomes, not an exactly-once claim.

Verify large binary pack/advertisement preservation, concurrent input/output, noisy stderr,
startup/exit/deadline/body-drop failures and descendant cleanup; real clone/fetch and push
against bare/worktree storage; declared-versus-actual ref refusal before effects; exact CAS,
atomic and partial acceptance; acceptance followed by process failure; and API admission
held through client disconnect/cleanup with confirmed publication signals. Extend the
existing guard to the now fully migrated Git route. The stale push-order check targets
workspace publication: require the actual service call after scoped authorization and
owner validation, with a missing-effect control; also guard receive admission ordering.
Governing: `git_execution`, `git_ref_updates`, `git_native_runner`, `git_outcomes`,
`git_retry`, `git_locations`, `git_guard`, network CAS/ref-scope/receipt admission and
runtime stream/mutation lifetimes. No governing meaning change is required.

## Remaining CLI mechanics and semantic identity

Complete the CLI boundary without changing its workflow policy. Existing remote URL
updates/create-if-missing configuration move behind owned GitRepo operations. Native
version/default-branch/HEAD/ignore/history probes and dev-mirror push use the same native
runner with CLI context, bounded observations, deadlines and existing cancellation.
Keep the dev-container mirror destination/layout and best-effort warning policy with
its owner; declare the one branch and fast-forward update instead of a raw push command.
Conservative spec checks still distinguish a new file/unborn repository from unavailable
Git or ambiguous storage. Preserve native transport variants, including unauthenticated
`git://` default-branch discovery. Resolve relative mirror destinations from the source
checkout and use the same absolute location for push and Docker copy. Extend process
signal ownership to every newly native-consuming CLI command family.

Delete global safe-directory writes from CLI materialization: current-user checkouts
need no global grant, and guest member trust already comes from runtime-owned config.
Do not replace the user's config or broaden repository trust. Verify external clone and
workspace-context behavior, and unchanged global config. Dispatcher-owned persistent
member configuration remains part of the later runtime migration.

The filesystem semantic engine has no production consumer of its implicit Git author
lookup in this checkout. Remove that backend probe; its constructor accepts the local
email explicitly and still captures provenance per engine, with missing email mapped
to typed unknown. Keep explicit hosted actor/executor constructors and their validation.
Do not create a new native operation for an otherwise dead author probe.

Governing: Git execution/native runner/outcome/retry/credentials/location/guard blocks,
CLI runtime/identity contracts, and existing provenance identity. This implements the
approved design; it introduces no new tracking or retry policy.

## Host cache, checkout and runtime configuration

The next native increment keeps cache identity, URL admission, token selection,
per-key/cross-process locking and the existing bounded 403 visibility recovery in the
host. SCM owns bare clone/full-branch refresh and exact checkout materialization.
The host supplies its selected executable search path, home/XDG/system/global config,
SSH agent/tool, HTTP proxy, CA and loader context;
SCM constructs isolated invocations and scopes credentials to the admitted URL.
Cancelling the awaiting materialization cancels Git, while an owned in-memory task
retains both cache locks through child cleanup. Its overall execution deadline stays
below stale-lock recovery age; this adds no retry or persistent worker.

Restore receipt validation, interruption reporting and fresh-root replacement stay in
the host. SCM fetches only the declared local tracking/receipt namespaces, verifies
receipt OIDs, restores attached/detached HEAD and binary dirty content, and clears
sequencer files. The optional parent fetch remains bounded and informational; confirmed
local effects are retained when a later restore stage fails. Host retained-root control
files are still discarded before SCM is invoked.

Task path selection and assigned repository cuts remain runtime decisions. Exact-root
trust-file serialization and exclusive-create/replace/ensure mechanics move intact to
SCM configuration operations. Split backend-free workspace path routing from optional
Git workflows so host/runtime crates can depend on their actual capabilities without
linking libgit2. Keep the existing public path-routing API compatible.

## Guest member operations and flush capture

The dispatcher keeps member selection, association policy, module admission, transport
lease lifetime, result projection and the existing flush receipt/recovery state. Its
Git adapter supplies a typed member context: runtime-selected executable path/loaders,
private config/home, owner author identity and provisioned process uid/gid. SCM owns
the corresponding environment, invocation-only index controls and hook suppression;
there is no public argument-vector or environment-map operation.

Complete SCM operations cover member facts/status/divergence, selected-path and whole
checkout checkpoints, exact leased publication, tracking refresh, switch-back and
parent update. Selected checkpoints retain unrelated staging, expected raw-blob checks,
index exclusion and exact ref CAS/compensation. Parent update retains the existing
hunk-level base-favoring replay and publication ancestry; scratch files remain under
the supplied private state root. Interruption uses bounded fresh cleanup to abort the
sequencer, retaining partial/unknown effect evidence. Dispatcher operation custody
retains admission and transport leases until the native runner finishes cleanup, even
when an awaiting caller is dropped.

Flush preflight limits, member-vector admission and receipt sequencing remain with the
dispatcher. SCM owns the immutable checkout snapshot, remote-tip observation/fetch,
bounded pack-size measurement and declared receipt-ref pair publication. Privileged
object writing completes descriptor-relative, no-follow object-directory handover
before operation custody is released, including failure/cancellation. The existing
flush state and bounded fixed-OID push recovery are reused; no new retry persistence
or automatic replay is introduced.

Runtime paths, excludes policy, member grants and relay leases remain owner inputs.
SCM serializes and atomically installs the private identity/trust/include configuration
and exact-checkout transport/upstream configuration. Interactive user Git remains its
ordinary tool; platform operations continue to use the isolated relay without placing
the incarnation token in repository processes. The dispatcher selects an exact member
credential directly in memory for its HTTP relay; the redundant shell helper is removed.
It supplied no independent authority or isolation beyond the same membership check.
The dispatcher also scrubs its configured secrets from native diagnostics before
logging or public serialization, including prefixes cut by bounded capture. SCM
validates literal branch arguments internally; association failures return outcomes
and release admission instead of relying on unchecked observations.

## Provider transport, App and issue implementation increment

The optional SCM `providers` feature does not enable the libgit2 or native backends.
`ProviderClients` constructs private shared HTTP mechanics and scoped App, App-user,
installation, repository and independent OAuth clients. Operation inputs carry the
selected credential and endpoint owner; token cache, JWT signing, installation binding
and user grant policy stay in API core. The App owner retains token-lifetime filtering.
Issue adapters bind the selected numeric/name repository and read/comment capability
before calling SCM. Their raw HTTP sharing accessor is removed; public DTO projections
remain compatible.

The transport has bounded connect/request/body lifetimes, a 2 MiB body bound, no
redirects and no replay. It retains observed status/request/rate evidence on later body
failure, so a confirmed write is Partial rather than Unknown. App inventory reads
stop with a clear error after ten pages rather than returning an incomplete inventory.
Issue operations retain their five-second budget and existing domain error kinds.
Pasted issue URLs must match the configured instance; a public-GitHub URL is not an
alias for the same repository name on an independently configured instance.

This increment does not complete AuthService/account/PAT, PR lifecycle/read/check or
platform-report migration. Their existing owners and recovery policies remain pending
work; the independent OAuth client is ready for the live AuthService migration.

## Final backend dependency closure

The shared contracts crate has no libgit2 feature or error conversion. SCM privately
maps backend errors into the existing owned error contract, preserving its message
and operation behavior. Aggregator API/CLI binaries retain backend access only in test
fixtures; the CLI's intentional SCM-backed workflows may still transitively link the
backend, while native host/dispatcher crate graphs do not. The API owner keeps its
configured executable override and fallback; SCM performs executable discovery without
spawning a separate lookup tool. Source and manifest guards cover every backend crate,
including newly added crates, while preserving legitimate test fixtures.

## Parked security finding

The user identified sibling-workspace branch/object exposure and asked whether Git
could reasonably enforce branch-level read permissions. After the user instructed us
to stop that extension if it was impractical, the proposed security amendment was
withdrawn. Git's [security guidance](https://git-scm.com/docs/gitnamespaces#_security)
requires a stronger repository/object-store boundary than narrow clones, hidden refs
or namespaces provide. Introducing isolated served repositories is a separate storage
and transport redesign, beyond this abstraction migration.

The existing `g5.security.mvp-exposure.objects` limitation therefore remains explicit.
This work does not claim that branch filtering fixes confidentiality. No security
implementation or destructive cleanup was performed; original fetch/restore semantics
remain governing for this migration. The finding remains recorded for separate work.

## Acceptance outcomes

- Production backend access/spawns occur only inside their declared adapters; public API
  types expose no raw handles, HTTP clients or generic command/environment controls.
  Native-only host/guest builds exclude libgit2; backend changes do not force caller changes.
- Existing reader/integration guards still protect their owners. New checks reject
  configured-URL provider bypass, typed-Git content-read bypass and missing-target false
  passes, while allowing private helper renames and legitimate composition/provider choice.
- Authorized content reads retain the selected pinned version, projection maintenance
  uses typed readers, and the full-context diff consumer is served without raw libgit2.
- Wrong old OID, unauthorized ref and conflicting per-run pair are refused without
  mutating the refused transaction's refs. Identical per-run replay is a no-op. Coupled ref updates retain their
  transaction scope; native partial acceptance reports only confirmed accepted refs.
- Push admission, receive/upload-pack, fork, remote-tracking merge, conflict resolution,
  flush/restore and GC retain their existing behavior. Selected-file checkpoints preserve
  unrelated staged/working changes and refuse a changed-version publication retry.
- Native process inspection confirms credentials absent from argv; config/URLs/errors
  contain no secrets. Global/system config is untouched; explicit member identity/config,
  temporary-index settings and host control-file isolation remain correct.
- Deadline/cancellation cleanup does not orphan the runner's child/streams or claim that
  a completed remote effect was rolled back. Outcomes distinguish refusal, partial and
  unknown effects with actionable, redacted information.
- A lost PR-creation reply does not cause a second create request; a rate-limit response
  returns a hint without hidden sleep/retry. An agent/human can inspect current state
  and explicitly retry through the existing flow. No new tracking schema or retry storage.
- A candidate credential can be validated and organizations listed before repository
  selection. Installation-token issuance does not recurse into credential selection.
  Missing user PR authorization cannot fall back to an installation/other-user token.
- Existing PR authorship, UI/CLI save/push/create/update, push refresh and partial-provider
  reporting remain intact. Explicit Update PR retries use existing evidence/current heads;
  failed title synchronization still requires explicit title resubmission. No pending columns.
- PR check reads retain repository/revision binding, bounded pagination/diagnostics and
  current admission; no read publishes, starts compute or reruns CI. Existing CLI polling
  is preserved without adding a server-side retry service.
- Owner-specific stub hosts receive all live provider/login/report requests intended for
  them; others are unaffected. Credentialed redirects and cross-owner pagination are
  refused. Public URL parsing honors the matching web base. Distinct layout fixtures
  resolve API repos, bare host caches, guest worktrees and dev mirrors correctly.

Use existing behavioral suites and focused boundary cases for changed contracts. Tests
may adapt to new interfaces while retaining their behavioral assertions; green source
counts alone are not sufficient. Current implementation verification and remaining acceptance gaps are recorded in the
[initial implementation review](ontology-review-git-abstraction.md).

## Risks and open source assumptions for coding

- **Scope inflation from stale evidence:** the audit covers an older base and some
  original inventory entries were tests, dead code or legitimate owners. Re-inventory
  production paths, including the landed PR/check/checkpoint operations, before migration.
- **Reused-operation semantics:** inspect actual authorization, ref preconditions, locks,
  transaction boundaries, cleanup and publication signals. A design-approved reuse does
  not establish compatibility. Resolve per-run replacement behavior against its callers.
- **Native-only dependency closure:** inspect Cargo features and existing g5d dependency
  guards. Shared types cannot pull in libgit2, and `g5-scm` cannot depend on runtime orchestration.
- **Context/config compatibility:** inspect supported binary versions, real credential
  channels, external-CLI lookup policy and global-config callers. Confirm each setting's
  intended owner and preserve protocol stdin and runtime PATH/loaders.
- **Client placement and live scope:** inspect which provider helpers combine protocol
  with SQL installation policy; locate a downward-only transport home. Confirm AuthService
  import/connect/discovery paths, capability support, OAuth call liveness and endpoint overrides.
- **Behavior changes from redirects and retries:** inspect current live policies; keep
  request deadlines and clear error mapping. The required new abstraction performs no
  automatic replay. Any separately governed existing recovery remains at its original owner.
- **Guard limits:** source contracts need calibrated fixtures and a maintained operation
  inventory; they cannot prove dynamic mediation or runtime authority by themselves.

## Design review disposition

The ontology and this handoff now resolve the ref/backend ambiguity, pre-repository
credential scope, authorized-reader preservation, location variants, error/retry ownership
and guard gaps. The deliberate second pass followed the invariant index to durable
reading, receipt/ref authority, the landed PR/check owners and selected-file checkpoint
contracts. It preserves their distinct limits and legitimate native execution paths.
No intent question blocks the design; the source assumptions above remain for coding.
Implementation and scoped review are complete. The final assessment below records
verification results and preserves unexecuted deployment/platform boundaries.

## Final provider migration implementation

Candidate PAT validation and account repository/organization listing use grants with
no invented repository identity. Login and SCM/App OAuth have distinct typed clients
sharing private protocol mechanics; the live owners keep identity/email linking,
stored-credential refresh/cache and authorization. PR lifecycle/read/check clients
consume repository/capability grants, with existing actor choice, publication/422
recovery and bounded revision consistency reads staying in their owners. Provider
rate limits do not enter the existing 422 reconciliation path.

Platform reports retain their App signing, token cache and durable outbox owner. Their
API and web bases are independent configuration (`G5_PLATFORM_ISSUE_GITHUB_WEB_BASE_URL`
defaults to public GitHub), so returned issue links and generated commit-pinned asset
URLs remain on the report destination, including Enterprise prefixes. Installation
issuance explicitly narrows to that repository name and issues/contents write, matching
the existing delivery contract; GitHub supports named repository and permission
narrowing in its [installation-token API](https://docs.github.com/en/rest/apps/apps#create-an-installation-access-token-for-an-app).
This adds no retry state, schema, worker or replay.

### Final review repairs

The independent final review found a reused public-GitHub-only repository parser and
origin policy below the configured-instance PR selector. The supported repository
instance is selected by GitHub App API/web configuration when an App is configured;
otherwise `G5_SCM_GITHUB_API_BASE_URL` and `G5_SCM_GITHUB_WEB_BASE_URL` select it
(defaulting to public GitHub). The same resolved endpoints drive SCM origin policy,
account integration and PR lifecycle/reads. Login and platform-report destinations
remain independent.
SCM carries that trusted HTTPS origin through a GitHub instance provider value,
parses attached clone identities against its selected web prefix, and preserves exact
origin/port credential checks. Instance selection replaces public GitHub for those
credentials, rather than broadening a token to both hosts. API App/read/check owners
must use the same selected instance; public client URL checks alone are insufficient
acceptance evidence. This implements `github_transport` and `git_credentials` without
introducing a new endpoint authority or relaxing repository authorization.

Other repairs provide all configured check secrets before diagnostic truncation,
interpret negative merge responses as typed refusals, bind report issue links to the
selected repository/resource/number, retain outcome guidance ahead of bounded owner
diagnostics, and restrict raw HTTP construction/delivery to the shared transport.


### Completion and verification scope

All planned implementation increments are complete on `design/git-abstraction`:
backend ownership and typed operations, native execution, API/CLI/host/guest migration,
scoped provider clients, private shared HTTP transport and calibrated ownership guards.
The final repair keeps configured GitHub origin, port and path prefix through both App
and personal credential admission; independent login/report endpoints remain separate.
Provider-name guards distinguish protocol dispatch from named composition, credential
policy, persisted-record validation and existing public-response presentation.

The [review ledger](ontology-review-git-abstraction.md) is the authoritative test receipt
and limitations record. Three fresh final-provider reviews were consolidated before
coherent repair, followed by a bounded independent origin/error repair check. No new
retry persistence or replay was added, and the parked branch/object confidentiality
extension remains outside this implementation. Live SaaS/Enterprise requests, full
VM/non-Unix deployment, encrypted SSH handshakes and the exhaustive cancellation/
partial-leaf matrix are not claimed by local fixture evidence.


### Follow-up abstraction audit

The user's follow-up asks whether alternative paths are consolidated, whether future
duplicates fail a check, and whether simultaneous in-platform merge/fork has real
consistency tests. Fresh independent discovery is retained in the existing review.
The implementation boundary needs further tightening: remove unused unchecked
writer/workflow APIs, make retained local ref changes express expected state, and
extend the structural guards to every SCM source and ordinary Rust reader/process
API spellings. Legitimate native/libgit2 and policy/context variants remain distinct.

Fork source capture must cooperate with the existing workspace publication fence:
select membership and any retained receipt, then read the complete current Git
head vector under the shared bounded fence. Use that same vector to establish
receipt currency and release the fence before provider or live-capture work. This
repairs the inherited mixed-vector interleaving under the existing complete-source
contract; it adds no cross-repository Git atomicity, new lock namespace or retry
state. Existing receipt retention/pruning limits remain separate. Deterministic
public merge/fork tests must verify Git refs, frozen manifest, child membership and
replay for coherent before/after captures. Guard calibration is necessary but cannot
prove arbitrary semantic duplication or macro/dataflow behavior.


The caller inventory also confirms that the old effectful structured-merge workflow
has no live consumer. Its resolver-injection port and compatibility attachment exist
only to support that unused workflow and its tests; they are removed together. The
canonical semantic resolver in `local_grounding` remains. Tests of retired workflows
are retired with them, rather than counted as coverage of current merge behavior.

Main integration at `fd2ad7ca` supersedes the audit's post-COMMIT compensation
choice with `g5.git.landing.commit-point`: run deferred database checks before
moving refs, restore partial Git writes under the original fence, and never
retract an accepted cut after releasing it. This closes the gap in which a fork
could capture a committed-looking vector before compensation reacquired the
fence. The prior design and test receipts remain in commit `a2fb3d49` and the
review; they are historical evidence, not proof of the combined implementation.

Fork admission uses main's shared bounded committed-cut reader and a single
captured vector for receipt currency and source refs. Retain main's lock ordering,
PR/VM/download cut checks, Base pull-only tracking, co-landed imports, provider
history acceptance and advisory behavior. Remove the retired protected-ref import
operation and tests, while keeping all surviving mechanics behind typed SCM
operations. No new retry state or automatic replay is introduced by this integration.

Committed-cut readers consume their read-only admission transaction before opening
the short fenced capture under the same scoped capability. Later metadata and
application phases reopen only after capture closes; merge still reauthorizes the
destination before landing. This preserves lock ordering without nested connection
acquisition, which can exhaust a bounded pool during concurrent fork/merge or reads.

One macOS native/libgit2 TLS test-order failure remains unresolved and is recorded
in the audit and credential metadata; the complete combined SCM suite is not claimed
green. Its endpoint and credential assertions remain unchanged.

## Second abstraction audit repairs

[Request](chat_intent/design-git-abstraction/request.md#second-abstraction-audit). A fresh
read-only audit of `6b402c95` found that execution and provider HTTP have single owners,
but several operations still have competing paths, and some acceptance outcomes lack
executed evidence. Repairs stay inside the retry constraint: no new retry state.

**Network Git is native-only (amends `git_native_runner`, `git_credentials`).** Every
operation that contacts a remote (ls-remote, clone, fetch, object download, push and
delete) runs through `NativeGit` in its caller's composition context. libgit2 remains
the implementation for local object, ref, diff and commit work only. Consequences:

- API services receive the API runner selected from `G5_GIT_BIN`; `GitRepo` network and
  push methods take that runner instead of constructing a CLI context. This repairs API
  lease pushes that ran with the ambient PATH and environment.
- Publication is one operation over an exact commit: fast-forward when the observed tip
  is absent or an ancestor, otherwise a lease on the observed tip. Create-only uses an
  absent-destination lease rather than a probe followed by a plain push. The branch-name
  variant, libgit2 refspec/staging pushes and their credential callbacks are retired.
- g5d-cli clone/fetch/push use the runner for every remote scheme instead of selecting
  libgit2 for non-HTTP remotes; the native layer already admits file, SSH and HTTP(S).
- Native deadlines, typed effects and the single injection site now cover all network Git.

**One checked ref-update path.** `apply_ref_commits` is the only libgit2 CAS; single-branch
updates delegate to it. Exact checkout pinning in g5d-cli uses checked operations rather
than an ignored unconditional branch delete. Unused competing variants (watermark CAS,
branch-name merge creation, unchecked deletes, dead history/working-tree helpers and
pass-through workspace-ops wrappers) are removed rather than kept as alternatives. The
ref-mutator guard scans every backend crate and names only existing operations.

**Provider consolidation.** Installation-token issuance, App JWT signing and token caching,
issue transport/error parsing, 403 classification and GitHub repository-identity parsing
each get one implementation; Adopt-Remote identity uses the configured-instance parser so
Enterprise prefixes do not enter stored identity. The unused credential-store helper that
persisted a token in a URL is deleted. Owner-specific error meaning stays in typed clients.

**Native internals.** One relay-expansion check, one Git-directory selection helper and the
existing `repo_roots()` replace inline copies. Divergent context policies remain distinct.

**Evidence gaps closed.** Verified request counts for no-replay provider cases (wiremock
`verify()` before `reset()`), a bounded-elapsed check for no hidden rate-limit sleep, a
dropped-connection PR create, an observable wrong-old-OID atomic receive, issuance
non-recursion, host credential rotation, typed layout fixtures and direct native tests for
flush/member transport operations. Guard calibration adds argument-level HTTP permits and
non-`reqwest` client detection.

Read operations that services call with already-pinned OIDs (review diffs, approval name
diffs, publication messages) remain with their existing authorized owners; the reader guard
names them explicitly so a new caller fails. Moving them into the workspace reader is not
required by `g5.architecture.git_execution` and would not change authority.

## Review summary

The second abstraction audit (four discovery passes) and one fresh independent
review of its repairs are recorded in the
[review ledger](ontology-review-git-abstraction.md#second-abstraction-audit--initial-findings):
native-only network Git, single CAS and publication paths, retired competing APIs,
provider consolidation, strengthened guards and discriminating evidence. Accepted
dispositions: non-HTTP CLI fetch parity with HTTP, SSH remotes without the transport
token, and the per-file issuance guard. DB-backed API suites and E2E await CI.


The implementation and abstraction audit cover typed Git/backend/provider ownership,
retired competing paths, structural guards, real fork/merge overlap and historical
compensation checks. Main's newer landing and capture contracts supersede
compensation. CI at `33fa4a7e` passes the combined Rust runtime suites, including
public fork/merge concurrency and constrained-pool committed-cut reads. Clippy
and latest-head E2E completion remain pending; see the review receipts.
The PR 294 main integration adds bounded self-review of native-memory eligibility,
capture refusal/retry and host restore, preserving main's consolidated ontology.
The external-review follow-up covers safe HTTP outcomes, provider failure mapping,
Enterprise endpoint composition, host restore policy, stream backpressure, E2E caller
compatibility and the CI dependency feature graph. Current evidence and the unresolved
macOS same-process TLS ordering failure remain
in the [review ledger](ontology-review-git-abstraction.md).
