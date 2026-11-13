# Shared PR write review — reviewer two

Independent read-only discovery review of the frozen write amendment, 2026-09-30.
No implementation-session explanation, author review artifact, or peer findings were
read. The provided request, pinned design, ontology, and current source were the inputs.
No production, test, ontology, or other review files were changed. The initial findings
and check results below were saved before reporting them to the coordinator.

## Initial findings

### F1 — P1: target read authority is not rechecked after asynchronous preparation

`backend/g5-api-core/src/services/workspace_pull_request_create.rs:120` checks target
access once. The transaction ends at line 171; commit and push then run asynchronously.
The fresh transactions at lines 176, 190, and 232 call `source_db`, which does not check
target authority. Neither the evidence-capture nor apply helper supplies this missing
check: `job_merge_service.rs:914` loads the workspace pair, and its loader at line 1882
checks identity and deletion only. The RLS policy on workspaces is organization-scoped,
not visibility/invitation-scoped (`001_lean_baseline.sql:8936`).

A member who initially has access to a target can lose that access while save/push is
in flight (for example, the target owner changes visibility from `org` to `invite_only`).
The request still captures the now-private target's repository evidence, creates a PR
against it, and can publish provider work. This is an inherited hole in the extracted
UI orchestration that is now also exposed to the new VM write adapter, not a defect in
the typed-token parser. It violates `g5.governance.pr.write`'s target-read requirement
and the current-admission/read obligations in `g5.security.admission`/`g5.security.read`.

Proposed repair: have the shared operation recheck target read access in the fresh
transactions before evidence capture and before admitting publication/provider effects.
Preserve the valid read-only target case. Do not repair only the CLI route.

Discriminating regression: begin an actual internal POST as a non-admin source owner
with a readable target; pause the native save/push peer, revoke only target visibility
or invitation, then resume. Require refusal before target evidence/PR/provider creation,
while preserving any completed source commit. Repeat through the public adapter and
retain a successful private-target case with a current explicit invitation. The existing
`vm_and_ui_pr_adapters_share_target_access_and_source_restrictions` test checks denial
before save only; it does not cover this boundary. This finding is established by source
inspection; this exact race was not executed in this discovery pass.

### F2 — P2: provider-phase admission or persistence failures are returned as an ordinary result

`backend/g5-api-core/src/services/workspace_pull_request_create.rs:232-278` logs every
provider-phase setup/service/summary/commit failure, then returns `Ok` with the earlier
summary at lines 281-284. A new local PR initially has `provider_status = 'local_only'`
(`coordinated_repo_publication.rs:366`), and that old summary is retained unless the
provider transaction and summary reload both succeed.

The new VM path adds a concrete failure case: stop, transfer, replacement, or lease
expiry after local PR commit makes the next `caller.source_db` refuse provider admission.
The local PR remains committed and the source branch may already be published, but the
response contains no indication that provider creation was skipped. A provider DB
failure produces the same misleading result; on a reused PR it can return old `open`
provider evidence as if the update phase completed. `g5d-cli/src/pr.rs:125-148` prints
the normal created/updated result and exits successfully because no partial-phase
outcome is available to render. The ordinary provider per-repository failure path does
record errors correctly; this finding concerns failures outside that recorded path.

This violates the explicit partial-failure reporting obligation in
`g5.governance.pr.write` and the design's shared UI/CLI recovery result requirement.
The log-and-return pattern is inherited from the UI route, but the new VM phase refusal
and the new command's reporting contract make it directly material to this amendment.

Proposed repair: expose an explicit incomplete/unknown provider-sync outcome with the
committed G5 PR identity, or return an actionable partial-failure error containing that
identity; preserve the local record and never retry the mutation automatically. Ensure
both UI and CLI consume the same outcome. Do not fabricate provider success or label a
failed provider phase `local_only`.

Discriminating regressions: (1) fence the VM between local commit and provider admission;
(2) force provider-state persistence/summary failure after provider activity. Assert the
returned outcome is visibly incomplete and includes the G5 identity; stored/provider
state remains recoverable, and a subsequent authorized retry reuses the open PR.
The current CLI mocked `providerStatus=error` test proves rendering of supplied errors,
not that this shared owner supplies them. This finding is source-established; those exact
phase-failure injections were not executed here.

## Coverage and obligation ledger

| Commitment | Actual path and discriminating check | Assessment |
|---|---|---|
| `g5.architecture.pr_flow`, `g5.governance.pr.write` — exactly one create/update flow | Public route `workspaces.rs:5082` and internal route `internal.rs:2704` both call `workspace_pull_request_create::create_or_update`; `PrCommand::Create` has the visible `update` alias. The same owner contains guards, native preparation, capture/apply, source publication, and provider synchronization. UI hook POST remains the same endpoint. | Conforms by call-graph inspection and public/VM reuse tests; no duplicate publication algorithm found. F1/F2 concern completeness of that owner. |
| `g5.security.owner`, `g5.security.admission`, `g5.runtime.lease`, `g5.security.login.service-identities` | Middleware permits the exact internal POST for incarnation tokens; public route requires `AuthenticatedUser`. VM source identity is compared, current owner/VM/epoch/lease/stop/transfer is checked, and native preparation pins the original VM/epoch. Tests reject user/host/VM crossing, wrong source, transitional/stopped/replaced/expired/transferred authority, and replacement during preparation. | The exercised VM boundaries pass. Source transaction is released before native callbacks. Provider-phase refusal is safe for effects but underreported (F2). Target revocation is missing (F1). |
| `g5.security.read`, `g5.security.tenant`, `g5.security.tenant.write` | Actor-scoped capability, tenant identity and target access helper; shared route tests deny unreadable target and read-only source before native work. Merge evidence validates matching org and repository vector. | Initial authority conforms; temporal target authority fails F1. No new membership-only mutation grant found. |
| `g5.governance.publish`, `.publish.readiness` | `prepare_pull_request_source` invokes commit then push before evidence; failed save/push stops downstream creation, retains committed work, and stopped dirty flush receipts refuse. Runtime calls re-admit the pinned incarnation. | Nine selected save/retry/incarnation tests pass. Real nested VM-agent execution and actual native commit/receive-pack remain unverified; test peers simulate native reports and one fixture advances the durable ref directly. |
| `g5.governance.publish.github-author`, `.publish.actor`, `g5.security.credentials.publication` | Provider service still chooses `CreatePullRequest` only for a new GitHub PR; `RepositoryCredentials` preserves membership, URL/binding/repository/permission checks and initiating-user grant; other operations retain their credential policy. CLI sends only G5 credentials and request data. | Mock-provider test passes for VM actor-authored creation, existing PR reuse through UI, and missing-grant recovery. Live GitHub and the complete external permission matrix were not exercised. |
| `g5.governance.pr.json` | Field selection runs before body/workspace/auth loading, create/update use a plain selected G5 object; body/body-file conflicts parse before transport. | Compiled CLI integration tests pass, including pre-mutation help/error behavior, body inline/file/stdin, both write names and no ambiguous-response retry. |
| `g5.governance.pr.read` | Read dispatch/path selection remains independent; create branch is separate before `select_output`. | Existing compiled read/JSON cases in `pr_commands` pass; this pass did not redo the prior full retrieval review. |
| Review-workspace exception and source restrictions | Shared `ensure_not_review_workspace` precedes save; review-specific handback remains separate. Mutable source, owner, non-base and distinct target checks remain centralized. | Actual VM review-workspace rejection and public/VM initial source/target restriction tests pass. Target may remain read-only. |
| `g5.design.minimal-mechanism`, `.change-locality`, governance ownership | One operation extracted from the former route; typed caller enum, existing services and credentials, thin transports, no schema/token store/new algorithm. | Boundary is justified by the user's explicit requirement. No unjustified new mechanism found. Policy rechecks should remain in this shared operation. |
| Worker guidance and packaging | Both `g5-pull-requests` skills are byte-identical, listed in both manifests, referenced by profile instructions; required skill files exist. Docs describe explicit target, auto-save, multiple provider outcomes, server credentials and uncertain-response recovery. Docker copies full profiles; Codex materialization validates/copies manifest skills recursively. | New skill's surface-sync check passes and neither skill is gitignored. Full legacy surface-sync is not clean; see checks below. No deployment/built-image verification performed. |
| `g5.quality.review.cases`, `.evidence.claims` | Runtime tests, source evidence and uncovered races are separately recorded here. | No claim of full live guest/provider completion. |

## Executed checks and limitations

Reused the available compiled test executables from the frozen implementation; did not
perform a fresh build. Successful invocations used `DYLD_LIBRARY_PATH=/opt/homebrew/lib`
and, for API tests, the configured local database URL as `TEST_DATABASE_URL`. The test
harness creates disposable databases and provider endpoints. Elevated sandbox execution
was needed for local loopback listeners; the initial non-elevated attempt failed from
that environment restriction and the missing dynamic-library path, not test assertions.

- `target/debug/deps/pr_commands-b256fc6a51d07b2c --test-threads=1`: 10 passed.
- `target/debug/deps/api_workspace_pull_requests-516222ff75651f80 create_pr_ --test-threads=1`: 9 passed.
- Same API executable, exact `vm_and_ui_pr_adapters_share_target_access_and_source_restrictions`: passed.
- Same API executable, exact `github_app_pr_creation_uses_the_actor_and_reuses_existing_prs_without_a_grant`: passed.
- `target/debug/deps/api_workspace_reviews-13ce537243cd522e`, exact `owner_rejects_while_review_workspaces_are_restricted`: passed.
- `python3 scripts/check_surface_sync.py --only g5-pull-requests`: one entry passed.
- Direct manifest/file checks: both profiles require the skill, all required skills exist,
  new skills are byte-identical, and `git check-ignore` reports neither new skill ignored.
- Full `scripts/check_surface_sync.py` reports 100 problems across legacy root/profile
  guidance (missing old clauses, absent root skills, divergent old guidance). These were
  inspected as a repository-wide check limit rather than attributed to this write change.
  `--only CLAUDE.md` also fails on old required root guidance; no new PR-specific pin error
  was observed. The PR-specific check above passes.

An initial direct API invocation without the explicit database variable was interrupted
after its first passing test while it probed fallback database URLs; the complete selected
suite was then rerun successfully with the configured URL. Logs are isolated under
`/tmp/pr-write-two-*.log`. No extra investigative source/tests were created.

Remaining evidence gaps: F1/F2's exact race/failure cases; real CLI invocation from an
active guest agent followed by actual native save/push callbacks; live GitHub/OAuth and
image/runtime-bundle deployment. A useful guest regression should invoke `g5d pr create`
from an already running `agent.turn` and verify dirty local content reaches the provider
branch without a callback lock stall. The existing test peer cannot establish that fact.

## Ontology assessment and proposed metadata changes

The saved governance diff includes the actual new `g5.governance.pr.write` and
`g5.architecture.pr_flow` blocks, the create/update JSON extension, and the retained
readiness amendment. Their governing meaning is consistent with the user's same-flow
request. No product-intent clarification or new governing decision is needed to repair
F1/F2; neither should be legitimized by weakening the ontology.

After consolidation/repair, update the currently `Planned` metadata for
`g5.governance.pr.write` and `g5.architecture.pr_flow`, and the create/update assessment
in `g5.governance.pr.json`, with current implementation and exact evidence. Retain the
real-guest/live-provider verification limits and any unresolved findings. The readiness
footer should distinguish native-peer API evidence from executed native guest saving.
No ontology edits were made in this discovery pass.
