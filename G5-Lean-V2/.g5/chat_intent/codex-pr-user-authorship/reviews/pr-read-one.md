# Independent PR retrieval discovery review one

Reviewed the uncommitted CLI PR retrieval amendment against HEAD `75b522e7` on
2026-09-30. This is a fresh, independent, full-scope discovery review. I read the
plan's governance section first, the saved request and amendment, the actual
ontology diff and paired metadata, all changed/new implementation, tests and docs,
and relevant unchanged consumers. I did not read peer reports or the earlier
implementation review, delegate, or change implementation/ontology/tests.

## Finding

### P1 — Admit the new internal read path in authentication middleware

- Changed location: `backend/g5-api-routes/src/routes/internal.rs:200-204`,
  particularly the handler registration at line 202.
- Related owner: `backend/g5-api-core/src/middleware/auth_middleware.rs:488-510`
  and `is_workspace_incarnation_internal_route` at lines 701-767.
- `g5d pr` selects `/api/internal/workspaces/{id}/pull-requests/info` whenever
  stable workspace metadata or `G5D_WORKSPACE_TOKEN` is present
  (`backend/g5d-cli/src/pr.rs:35-39`). The new router entry exists, but its exact
  GET path is absent from the incarnation-authentication allowlist. The auth
  middleware therefore rejects it as an unknown internal path with 401 before
  `read_internal` or its current-incarnation checks run. It is also not a
  host-only path (`is_host_only_internal_path`, line 1076). Consequently every
  PR command in the normal VM/provided-agent environment fails, including both
  stored reads and live GitHub reads.
- Governing obligations: `g5.governance.pr.read` current-workspace retrieval;
  `g5.security.admission` typed current VM authority; `g5.design.enforcement`
  working, consistently mediated entry paths. This is a missing allowed case,
  not an authorization bypass.
- Proposed repair: add only the exact GET route with UUID validation to
  `is_workspace_incarnation_internal_route`; retain the route's owner/VM/epoch/
  lease checks. Add the path to the middleware's allowed-route test and keep
  POST/invalid-workspace-ID variants forbidden.
- Discriminating regression: execute
  `stored_reads_enforce_visibility_and_current_workspace_incarnation` through
  the real application middleware. Its first internal request at
  `backend/g5-api/tests/api_pull_request_reads.rs:258-259` should currently
  receive 401 instead of 200. After repair require the valid request to return
  200 and the existing foreign-workspace/expired-lease cases to return 403.
  Also exercise a live provider selector through the internal route and prove
  an invalid incarnation issues no provider request. CLI wiremock tests alone
  cannot detect this omission because they replace the API server.
- Evidence classification: confirmed by source control flow; this reviewer
  did not independently execute the database-backed API suite.

## Commitment and coverage ledger

| Commitment | Actual path and discriminating case | Assessment |
|---|---|---|
| `g5.governance.pr.read`: current/default and explicit selection | `pr::execute`, `request_path`, `select_output`; stable metadata, VM env, classic workspace config, explicit UUID; numeric/URL and ambiguous repo handling | CLI transport/format tests pass against mock HTTP; real VM entry is blocked by the finding. Classic config and stable metadata defaults inspected, not independently exercised. |
| `g5.governance.pr.read`: stored versus live truth | API `source` discriminator; `render` stored-state label; provider mapping preserves null fields | Source supports distinction; CLI tests verify stored-state rendering and GitHub JSON. |
| `g5.governance.pr.read`: no PR/domain mutation or compute start | Stored path calls `list_workspace_pull_requests` → `list_prs` → `attach_provider_repos`; live path uses GETs after credential resolution and closes the read transaction | Source has no PR/domain writes or compute admission. Existing App-token minting/cache and personal-token resolution remain permitted credential effects. API tests inspect workspace/PR state but were not independently executed. |
| `g5.security.read`, `.read.durable`, `.tenant`, `.tenant.rls`, `.tenant.write` | Public route uses `scoped_db_for_workspace`, reader visibility checks and scoped queries; repo lookup joins attached rows and rejects unmatched names | Preserved on inspected source paths; org reader/private outsider/cross-org tests exist. App reader and internal live-lookup runtime cases remain gaps. No new write grant. |
| `g5.security.admission`, `.login.service-identities`, `.credentials.platform-token` | Internal adapter uses `scoped_db_for_workspace_incarnation`; it checks workspace/org/owner/VM/epoch/status/lease | Checks are correct for selected durable-read policy on inspection, but unreachable for this route until finding is fixed. API test includes current, foreign workspace and expired lease; replaced VM/epoch/owner cases are not newly exercised. |
| `g5.security.credentials.publication` | `RepositoryCredentials::resolve(ReadPullRequests)`; App binding/coverage/capability owner and personal SCM reader connection; only derived provider fields cross API boundary | Preserved on inspected paths. User URL cannot redirect request: fixed provider base, attached canonical repo, no redirects, sanitized GET errors. Read capability narrows App token to repository IDs and `pull_requests:read`. |
| `g5.governance.publish.actor`, `.publish.github-author`, `.publish`, `.publish.readiness` | Existing creation/lifecycle policy unchanged; only `github_repository_name` visibility changes and new read submodule uses read operation | No regression found by incremental source inspection. Earlier publication/runtime evidence not rerun or reinterpreted here. |
| `g5.architecture.governance_owner`, `g5.design.change-locality`, `.minimal-mechanism`, `.concerns`, `.reuse`, `.minimal-api`, `.enforcement` | CLI presentation → shared route adapter → PR owner → existing credential owner | Narrow additive operation with no schema/token store/background sync. Separate read GET implementation is justified by existing lifecycle reconciliation side effects and redirect policy. Missing middleware registration is the one found integration defect. |
| `g5.quality.review.cases`, `.evidence`, `.evidence.fakes`, `.evidence.claims`, `.evidence.unknown` | Reviewed tests at CLI, selector and real route boundaries | Three CLI transport tests independently passed; source-only and unexecuted API/live-provider boundaries remain explicitly separate. |

## Checks and limits

- `DYLD_LIBRARY_PATH=/opt/homebrew/lib target/debug/g5d pr --help`: passed;
  current binary exposes list/view/status and workspace override.
- Ran the already-built
  `target/debug/deps/pr_commands-b256fc6a51d07b2c --test-threads=1` with
  `DYLD_LIBRARY_PATH=/opt/homebrew/lib`: 3 passed, 0 failed. The first sandboxed
  attempt could not bind mock-server ports; rerunning with approved local
  network access succeeded. Each test uses its own `TempDir`/mock server.
- No Cargo build, shared test database mutation, provider network call or
  production mutation was performed. The first direct CLI invocation needed
  the existing Homebrew library path; that environment issue was resolved.
- Read all new API tests, including attached identity/read-token narrowing,
  URL/ambiguous/uncovered selection, provider error redaction, no redirect,
  stored visibility/incarnation and non-App reader credentials. Did not run
  them while the parent's API validation was active.
- Recommended remaining checks: real middleware internal success plus invalid
  token variants; live internal selector with no provider request on rejection;
  malformed/timeout provider response and missing personal credential; stable
  metadata and classic config default-workspace invocation. These are evidence
  gaps, not additional asserted code defects.
- Live GitHub behavior, real OAuth state and a deployed guest invocation remain
  unverified. No performance/whole-system security claim is made.

## Proposed ontology follow-through

After the coherent repair and API checks, update the paired
`g5.governance.pr.read` footer from planned to implemented with exact CLI/API
evidence and remaining live-provider limits. Add the new read owner to the
Git/governance public-operations navigation if maintaining full module API
coverage in this change. Record the checked read/credential/admission paths in
the relevant existing metadata only where this scoped evidence changes its
assessment; do not replace whole-system gap statements with a narrow pass.
No change to the governing behavior is required by this finding.
