# Independent review one — PR authorship and shared GitHub connection

Read-only review of the frozen working-tree changes on `codex/pr-user-authorship` compared with `main`, including untracked implementation files. Inputs were request.md, ontology-plan.md, actual ontology/code/test diff, ontology-review skill, authoring design-review guidance, and relevant governing ontology. I did not read review.md, implementation-session explanations, or peer reports. No production or ontology files were edited. Test resources were isolated by the existing TestApp/provider harness.

## Initial findings

### R1 — P1: Bind the outgoing PR repository to the repository authorized for the user token

Changed location: `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:904-917`, especially the creation credential request using only `candidate.git_url`; related new credential path: `backend/g5-api-core/src/auth/repository_credentials.rs:241-257`.

The credential owner checks binding coverage and current user access for the repository derived from `repos.git_url`, then returns the broad GitHub App user token. However, the PR consumer sends POST to `candidate.provider_full_name` (`workspace_pull_request_provider_lifecycle.rs:1614-1617`), which is preferentially taken from the independent `repos.provider_full_name` field at lines 1516-1518. These identities are never compared. Import accepts independently supplied `clone_url` and `full_name` (`g5-api-routes/src/routes/auth.rs:1537-1544`; `auth_service.rs:2474-2489` validates the origin only, then stores both fields). This is reachable persisted state, not merely an impossible corrupted DB premise.

If A is bound but the stored provider name names B, a successful existing-PR lookup for B followed by an empty result causes creation in B using the actor's broad token even though only A was checked against the G5 binding. The prior repository-scoped installation token constrained the final write to A; the new user token removes that constraint. A mismatched public B, for example, can be readable during discovery while the user/App token has wider write access than the G5 binding permits. The branch and PR metadata can then refer to different repositories, also risking disclosure of generated PR content to the wrong destination. Real GitHub exploitation was not exercised; the missing binding at the authoritative outgoing request is directly visible and reproduced by the supplied protocol test.

The newly added lifecycle test itself demonstrates this mismatch: coverage contains `g5-test-{org}/api` for ID 1002 (`api_workspace_pull_requests.rs:1256-1257`), the fixture stores that clone URL with `provider_full_name='acme/api'` (lines 1402-1414), and the test successfully POSTs the user token to `/repos/acme/api/pulls` (lines 1296-1299). I ran this test successfully. Thus it currently proves token selection while accepting a forbidden destination mismatch.

Governing obligations: `g5.governance.publish.github-author` preserves binding repository restrictions; `g5.security.tenant` and `g5.security.admission` require authority for the actual resource; the plan explicitly requires repository identity validation before grant use.

Proposed repair: derive/validate the provider request destination from the same canonical identity authorized by the credential owner, rejecting disagreement before any PR provider operation. Prefer an invariant-preserving typed credential/target operation or a narrowly scoped candidate identity validation rather than trusting a loose stored name. Adjust the happy-path fixture to one identity and add a mismatch test asserting no POST to B with the user token.

### R2 — P2: Preserve the nested destination when the connection page first requires login

Changed location: `frontend/src/App.tsx:291-296`; related `frontend/src/lib/githubConnection.ts:11-12` and `GithubConnectionCard.tsx:20-28`.

When an unauthenticated user opens `/account/github?returnTo=%2Facme%2Fwork%3Fview%3Dactivity`, RequireAuth stores that complete route and sends them to login (`useAuth.tsx:647-675`). After successful platform login, LoginPage passes this route into the new after-login card. The card immediately runs `githubConnectionReturnPath`, which treats any `/account/github` path as recursive and replaces it with `/`. Both connecting and skipping therefore lose the intended `/acme/work?view=activity` destination. A user who already has a grant is immediately sent to `/`, also bypassing the connection management page they requested. This occurs for a shared connection deep link or an expired platform session and violates the promised destination preservation.

Exact helper counterexample executed with Node against the real source:

`githubConnectionReturnPath(githubConnectionPath('/acme/work?view=activity')) === '/'`.

The full RequireAuth/login browser transition was inspected, not executed end to end. The existing App login tests cover a bare login/default workspace only and do not exercise this case.

Governing obligation: `g5.settings.github-connection` and the central connection amendment require optional authorization after login while preserving the intended destination.

Proposed repair: recognize an intended connection-management route in LoginPage and navigate to that already-authenticated route, or safely unwrap and preserve its validated nested destination. Retain the recursion/open-redirect defenses. Add login-required connection route tests for connected and disconnected users, including the eventual skip/connect destination.

## Checks and evidence

- Frontend: focused Vitest run of `GithubConnectionCard.test.tsx`, `githubConnection.test.ts`, `App.test.tsx`, and `ActivityCommentComposer.test.tsx`: 4 files, 46 tests passed. There were existing jsdom canvas / Node localStorage and Browserslist warnings; no test failures.
- Backend: reused the already-built `api_github_app_bindings-b919c716a60e1148` binary with isolated local TestApp databases and restricted runtime roles. Three `pr_creation_` tests passed: initiating actor/no teammate or personal fallback, refresh plus 401/403/404/503 handling, and non-App personal SCM compatibility.
- Backend: `repository_credentials_never_fall_back_after_app_denial` passed (one test). CreatePullRequest is explicitly included in the insufficient-permission operation matrix. Inactive and uncovered assertions in that test use ReadGit and establish shared-guard evidence, not direct new-operation execution.
- Backend: reused `api_workspace_pull_requests-516222ff75651f80`; `github_app_pr_creation_uses_the_actor_and_reuses_existing_prs_without_a_grant` passed (one test). It exercises the real lifecycle owner and persisted provider status with a protocol fake, including actor bearer token and reuse after deleting the actor grant. It also demonstrates R1's accepted repository mismatch.
- `git diff --check` passed.
- Node execution of the real return-path helper demonstrated R2's destination loss.
- Frontend build typecheck was started and its result will be recorded in an addendum.
- No backend rebuild, live GitHub call, real OAuth browser flow, or deployment was performed. Reused backend artifact results are distinct from a fresh current-source compilation.

## Commitment coverage, including no-finding obligations

| Commitment | Actual path and counterexample/check | Initial result / limit |
|---|---|---|
| `g5.governance.publish.github-author` | New CreatePullRequest operation; acting_user(actor); lifecycle POST bearer assertion; missing grant with another person's grant and personal token present | Actor selection and no fallback pass; repository restriction violated by R1 |
| `g5.governance.publish.actor` | Server resolver and GitHub request; refresh and access-probe 401/403/404/503 tests | No separate finding; real GitHub account attribution/write permission matrix unexecuted |
| `g5.governance.publish`, `.readiness` | Existing route prepare/apply, source publication and lifecycle source-blocked preflight gates remain unchanged | No regression found by inspection; dirty/unpushed readiness route not re-executed |
| `g5.security.admission` | Workspace create route scopes/owner checks and actor passed through; resolver rechecks org membership | Membership-removal check passes; actual provider resource mismatch is R1 |
| `g5.security.tenant`, `.tenant.write` | Scoped repository org/ID lookup, expected URL equality; caller owns write authorization | No newly introduced tenant-membership bypass found; destination identity gap is R1 |
| `g5.security.credentials.publication` | Encrypted existing grant store; token only enters server GitAuth and provider HTTP; sanitized access-probe errors | No token exposure found in changed path; token debug/error assertions pass |
| `g5.security.login` | Existing identity callback continues to `/login`; separate App user-connection endpoints | Sign-in remains separate; no automatic grant inference or new login provider |
| `g5.settings.github-connection` | Shared card/hook in Account and login; authenticated page; PR/comment shared links; actor-keyed query | Normal connected/disconnected, connect failure, reconnect label, disconnect, callback-failure rendering covered; intended path after login-required connection route fails R2 |
| `g5.architecture.governance_owner` | Lifecycle asks repository credential owner for a distinct creation operation; grant storage stays in existing owner | No additional ownership defect found; canonical target should be fixed at its owner for R1 |
| `g5.design.minimal-mechanism`, `.change-locality` | One shared hook/card and recovery route; no schema or push/merge credential rewrite | Scope is justified; no unsolicited mechanism identified |
| `g5.architecture.recovery` | Existing PR discovery before grant resolution; 422 reconciliation uses selected user token; persisted statuses retained | Existing PR reuse passes; no new duplication defect found; real ambiguous provider failures unexecuted |
| `g5.external-issues.publication.automation`, `.drafts`, `.broker.credentials` | Only comment connection navigation changed; comment broker/draft ownership unchanged; navigation test passes | No new bot substitution or draft publication found; full comment reconnect round trip unexecuted |
| `g5.quality.review.cases`, `.evidence.claims`, `.evidence.unknown` | 51 focused tests, source tracing, one helper execution, exact limits above | Available checks pass but do not discharge R1/R2 or live end-to-end outcomes |

## Missing evidence and metadata recommendations

- Add discriminating target-mismatch and login-required deep-link regressions for R1/R2.
- The actual CreatePullRequest new-operation cases for inactive/uncovered binding, missing/expired grant at the lifecycle API's persisted action-required row, and PR recovery link rendering are not directly exercised by the added focused tests. Shared guard inspection supplies partial evidence only.
- Full browser OAuth success/cancel/error, connection query isolation across user changes, disabled-deployment shared page, and reconnect followed by PR/comment retry remain unexecuted.
- The changed metadata footers still say Unassessed/being implemented. Consolidation should update both changed block entries with actual paths, test evidence, R1/R2 status, and live OAuth/GitHub limits. Relevant existing actor/publication credential/login/minimal mechanism entries can be scoped to the reviewed paths without implying exhaustive coverage. Preserve the source-of-truth contracts; neither finding requires weakening ontology.
- The saved plan's ontology diff matches the current two added blocks and metadata. Its high-level central connection amendment is sufficient; after fixes, record only consequential identity/return-path decisions in place.

Implementation is not ready to close while R1/R2 remain. Review discovery is complete.

## Check addendum

`frontend/node_modules/.bin/tsc --project tsconfig.build.json --noEmit` passed with exit code 0 and no diagnostics. No source or ontology changes were made during this review.
