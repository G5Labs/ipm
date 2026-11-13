# Review: issue #387 implementation (phases 1-4)

Self-review in the implementing session, not independent. Scope: commits `e14b3f7b`..HEAD on `issue-379-g5d-pr-cannot-read-inline-review-comments` against the [plan](ontology-plan.md). Phase 1 and 2 were committed before a VM loss; phases 3 and 4, their docs and the fixes below were redone after it.

## Initial findings

| # | Governing | Path | Finding | Consequence |
|---|---|---|---|---|
| 1 | `g5.governance.pr.write`; stable API error codes | `workspace_pull_request_create.rs::source_save_failed` | Save/push failures were rewrapped as a generic 409 `CONFLICT`, dropping `WORKSPACE_NOT_RUNNING`, `PR_SOURCE_PROVIDER_ADVANCED` and their structured details | 5 API tests failed; any consumer of those codes or details broke |
| 2 | — (tests) | `api_workspace_pull_requests.rs` provider tests | The no-replacement check reads a recorded provider PR; three tests never mocked that read | 3 tests failed with a 404 |
| 3 | `g5.governance.pr.body` ("the UI's description override") and the human's UI/CLI alignment answer | `PrepareToShipModal.tsx` | The description was read-only once a GitHub PR existed and was never sent | The UI could not do what `g5d pr update --body` does |
| 4 | — | `PrepareToShipModal.tsx::handleCreate` | `existingPullRequest` used but missing from the hook dependencies | Stale `mode` after the PR list changes |
| 5 | `g5.governance.pr.write` | ontology, `g5d pr update` help, docs | "`update` never creates a provider PR" is false: update opens one for a repository without one, which recovery after a failed first publication needs | Contract and help contradicted correct behaviour |
| 6 | — | `g5d pr --help` | Exit table lacked 5 and 7 | Discoverability |
| 7 | — | `docs/g5d-pr.md`, `AGENTS.md` | Phase 2 docs were lost with the VM | Docs described the old alias semantics |
| 8 | `g5.governance.pr.body` | tests | No test for refusing an over-limit description before saving | Unverified refusal |
| 9 | — | `AppError` | The fix for 1 first grew `AppError` past clippy's large-`Err` budget | clippy failed |
| 10 | — | phase 2 test | `drop(post)` on a `Copy` closure | clippy failed |

## Fixes and rechecks

- 1: `AppError` gains a one-byte `FailureKind` rendered as `details.kind`, with the original status, code and details kept (9: `request_id` boxed so `AppError` stays 120 bytes). g5d reads the kind from a 409 or 422. Rechecked by the incarnation tests asserting `WORKSPACE_NOT_RUNNING` plus the kind, `update_pr_publishes_tracked_sources…` asserting 422 `PR_SOURCE_PROVIDER_ADVANCED`, and compiled CLI tests mapping both to exit 4.
- 2: mocks state that the recorded PRs are still open; the tests then reach the credential and publication paths they were written for.
- 3, 4: description editable and sent only when edited; deps fixed. Modal test asserts the edited body and `mode: update`; a page test asserts the dialog's Update PR sends `mode: update` and Create none, and fails with the page wiring removed.
- 5: ontology, help and docs amended (see the plan's amendments).
- 6, 7: help and docs restored; compiled CLI help test asserts 5, 7, the push-then-watch loop and the provider ref.
- 8: API test `explicit_pr_description_over_the_limit_is_refused_before_saving`.
- 10: removed.

## CI findings after the review

The self-review ran targeted suites only. Cloud Build on `4a1f4628` (and the two heads before it) failed ten tests the review had not run:

| # | Check | Finding | Fix |
|---|---|---|---|
| 11 | `routes::guards` | `.expect()` and raw `Uuid` path IDs in `routes/pull_request_conversation.rs` | Validation error instead of `expect`; `Path<WorkspaceId>` |
| 12 | `route_handlers_use_expected_api_scopes` (also failed the e2e value-loop lane) | The two conversation handlers had no API scope contract | Contract: `WorkspacesWrite`, the issue-comment writes' scope |
| 13 | `owner_or_base_admin_admission_keeps_owner_and_base_admin_checks` | Phase 2 moved the Base subject query into `workspace_mutation_policy::base_admin_subject_org`; the guard still looked inline | Guard requires the call and checks the query's Base, live-workspace and PR-target conditions in the service |
| 14 | `every_context_file_has_its_reviewed_home` | `workspace_pull_request_conversation.rs` and `_freshness.rs` had no home | Both are Git context, like their siblings |
| 15 | `provider_dispatch_is_confined_to_selected_owners` | The incarnation route matcher's fixed `github` path segment read as provider dispatch | Classified as route matching |
| 16 | `all_backend_provider_protocol_uses_its_owner` | The internal client's gateway-only create retry is raw HTTP to G5's own API | Allowlisted beside the client's other G5 API helpers |
| 17 | `migration_045_owner_cleanup`, `migration_066_approval_phases` populated upgrades | 074 replaced `g5_issue_comment_publication_is_frozen`, which the restricted migrator does not own when the pre-045 schema was built by another role | 074 adds its own trigger freezing `destination` and `target_thread_id` and leaves the old function alone; the conversation API test asserts both are frozen |
| 18 | g5d `terminal_runs_g5d_pr_directly…` | Its create mock expected the pre-phase-2 body without `mode`/`replace` | Mock expects the current body |

## Commitment accounting

| Commitment | Path | Check | Result |
|---|---|---|---|
| `pr.read.freshness`: stale once target moves; conflicting paths; never recompute | `workspace_pull_request_freshness.rs`, `read_pull_request_info`, `pr.rs` | API test refreshes into a conflict, moves the target, asserts `stale`, per-repo `targetMoved` and an unchanged stored preflight | Pass. Unreadable mirror (`stale: null`) untested |
| `pr.read.freshness`: computing mergeability re-read briefly | `pulls.rs::read_pull_request` | SCM test: null→false resolves; persistent null stops after 4 reads under 5 s; closed PR read once | Pass; only CLI reads call it (inspected) |
| `pr.conversation` | `conversation.rs`, conversation service/routes, ledger 074 | `api_pull_request_conversation.rs`, SCM and compiled CLI tests | Pass; live GitHub mutations unverified |
| `pr.write`: update requires an open G5 PR; no silent replacement; `--new` | create/lifecycle services | API merged-outside test | Pass |
| `pr.write`: distinct exits, per-repo report, gateway retry | `pr.rs`, both clients | Compiled CLI tests (HTML 502 retry, exits 4/5/1) | Pass |
| `pr.write`: retry safety (provider sync serialized) | `synchronize_provider_pull_requests` PR-row `FOR UPDATE` held to commit | Inspection only | No concurrent-retry test |
| `pr.write`: follow-push never replaces | `create_or_update_one` recorded path | Inspection: a recorded completed PR is refused | Not runtime-tested here |
| `pr.body` | `update_pull_request`, lifecycle, modal | API create/update/omission, refusal test, modal test | Pass; derived over-limit bodies still fall back (pre-existing gap) |
| `pr.checks.logs` | none | — | Not implemented: needs CI-side publisher and a checks-only GitHub App |
| UI = CLI alignment | modal, page | modal and page tests | Pass |

## Remaining gaps

| Governing ontology | Gap | Proposed action | Uncertainty / blockers |
|---|---|---|---|
| `g5.governance.pr.checks.logs` | No CI-side log-tail publisher | Pub/Sub notifier on `cloud-builds` under its own identity, posting a failed check with the redacted tail via a checks-only GitHub App | Needs a human to create/install the App; Terraform and Cloud Run deploy unverifiable here |
| `g5.governance.pr.read` | Optional `files`, `commits`, `labels`, `statusCheckRollup` | Follow-up issue | Lowest priority in the plan |
| `g5.governance.pr.body` | Over-limit derived/project-doc body still falls back | Report it like the explicit refusal | Pre-existing; outside the explicit-description rule |
| `g5.governance.pr.conversation`, `pr.write` | Live GitHub, App permissions, deployed guests | Exercise on QA after deploy | Needs deployment |
| `g5.governance.pr.write` | Overlapping retry not runtime-tested | Concurrency API test with a held provider response | Inspection shows the PR-row lock |
