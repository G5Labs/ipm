# Independent review three — PR user authorship and shared GitHub connection

Scope: frozen working-tree changes and untracked implementation on `codex/pr-user-authorship`, compared with `main`. Fresh independent, read-only review. Read the review skill, saved request, full pinned `ontology-plan.md` (starting with governance), actual ontology diff, cited current blocks and metadata, changed source/tests, and affected credential, authentication, import, provider-publication and comment consumers. Did not read `review.md`, other reviewer reports, or implementation-session explanations. No delegation and no production/ontology edits.

## Initial findings (saved before sharing)

### P1 — Bind the PR API destination to the repository whose user credential was authorized

Changed location: `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:904-938`, especially the new credential request at 905-910. Related new credential return: `backend/g5-api-core/src/auth/repository_credentials.rs:241-257`.

The new path authorizes `candidate.git_url` and its covered numeric GitHub repository ID, then returns the person's broad GitHub App user token. The POST still uses independently stored `candidate.provider_full_name` (`load_provider_candidate`, lines 1516-1526; `create_pull_request`, lines 1613-1618). There is no identity comparison. `AuthService::import_repos` accepts `clone_url` and `full_name` independently from the import request (auth_service.rs 2467-2548); origin validation does not make those repository names equal. This is therefore a reachable mismatch rather than only an impossible corrupt row.

A covered repository A can authorize and receive the branch push while the subsequent user-token POST targets B. B can be a public repository or another repository reachable by the same user/App that is outside this G5 organization's selected binding coverage. Discovery's installation token does not establish the POST's authorization: listing public PRs can succeed without repository permissions. Previously the creation token was narrowed to A; changing it to a user token removes that final repository restriction. The current new lifecycle test actually sets up this mismatch: coverage and git URL are `g5-test-{org}/api`, while `provider_full_name` and the POST endpoint are `acme/api` (api_workspace_pull_requests.rs 1256-1257, 1282-1295 and fixture 1402-1414).

Consequence: PR publication can occur in a different repository than the selected, authorized repository, including publishing derived title/body text there under the user's identity. This violates `g5.governance.publish.github-author`'s retained binding restrictions and the plan's repository-identity requirement.

Proposed repair: make the checked repository identity supply the provider API destination (or compare/resolve the destination to the checked GitHub repository ID and reject mismatch before any provider mutation). Do not trust an independent full-name field after credential selection. Change the positive fixture to use one consistent identity and add a regression with covered clone URL A plus full name B which proves no POST to B, including the public-list-success case. Keep a valid matching URL/full-name variation passing.

Evidence: source trace plus the existing mock lifecycle test; live GitHub exploit is not executed. Official GitHub documentation confirms public pull-request listing can be used without the normally required token permissions: https://docs.github.com/en/rest/pulls/pulls?apiVersion=2022-11-28#list-pull-requests .

## Checks and evidence available at initial save

- `git diff --check`: passed.
- Frontend focused run: `npm test -- src/components/settings/GithubConnectionCard.test.tsx src/lib/githubConnection.test.ts src/components/external-issues/ActivityCommentComposer.test.tsx src/App.test.tsx`: 4 files, 46 tests passed. Existing non-fatal jsdom canvas, Node and Browserslist warnings appeared.
- Reused current compiled integration binaries (mtime after changed source, and listed expected new tests), avoiding an expensive rebuild. Sandboxed first attempts were blocked by local socket/Postgres access; escalated runs permitted local isolated test resources.
- Lifecycle test `github_app_pr_creation_uses_the_actor_and_reuses_existing_prs_without_a_grant`: passed, 1 test in 41.27s. It exercises provider service, token headers, persisted open status and existing-PR reuse. The mismatched fixture noted above is accepted by this path.
- Credential tests `pr_creation_`: running at initial save; first refresh/revocation/access test passed. Results will be appended without altering initial findings.

## Commitment coverage, including no-finding obligations

| Commitment | Actual path and counterexample/check | Result or gap |
|---|---|---|
| `g5.governance.publish.github-author`: initiating user only | Create operation invokes `acting_user(actor)`; another user's grant and personal token cannot substitute; test `pr_creation_requires_the_initiating_users_app_grant` | Source-inspected; focused runtime pending initial save |
| Same block: current repository access and binding restrictions | Membership, captured git URL, active binding, full-name coverage, capability preflight, then user-token GET by repository ID | Repository selection itself preserves refusal; final POST destination mismatch is finding above |
| Same block: absent/revoked/expired grant | `acting_user` preserves row locking/encrypted refresh, no-grant/invalid refresh maps connection-required; access probe 401 maps same marker, 403/404 deny, transient error remains retryable | Source-inspected; refresh/probe test passed; missing/revoked full public-route persisted status not executed |
| Same block: non-App compatibility | Resolver falls through to existing acting-user personal SCM owner only without binding | Source-inspected; focused runtime pending initial save |
| Same block: existing PR author and push policy | Discovery/push still use `WritePullRequests`/`WriteGit`; creation credentials only requested after no existing PR; 422 retry reuses user token | Service integration creation/reuse passed; concurrent provider race with real GitHub unexecuted |
| `g5.governance.publish.actor` | Public route forwards `user.user_uuid()` to provider owner; token is selected per invocation | No new actor substitution found; endpoint-provider-destination issue remains as above |
| `g5.governance.publish`, `.readiness` | Existing route `prepare_pull_request_source` and coordinated prepare/apply precede provider invocation; source-blocked preflight refuses provider work | Unchanged behavior inspected; no new readiness bypass found, dirty live workspace end-to-end check not run |
| `g5.security.admission`, `.tenant`, `.tenant.write` | Workspace create route still checks scoped access, mutation policy and owner; credential resolver checks membership and org-scoped repository | No new role or cross-tenant-ID bypass found; test deletes membership and expects denial; destination restriction defect separately applies |
| `g5.security.credentials.publication` | User tokens remain in backend resolver/App client/provider HTTP requests; UI receives status/login and redirect URL only | No new guest/browser credential propagation found; access-check failures discard provider response body and debug auth redacts token |
| `g5.architecture.governance_owner` | Provider lifecycle asks credential owner for distinct creation operation; grant storage/refresh stays with existing owner | Appropriate reuse, no new SQL/token store in PR service; no separate architecture defect beyond destination binding |
| `g5.design.minimal-mechanism`, `.change-locality` | One new repository operation, one access probe, shared hook/card/page and recovery link; other App credential modes untouched | Minimal scope is justified; no unsolicited workflow/schema/provider change found |
| `g5.settings.github-connection`: optional login and existing identity | Auth callback still redirects to `/login`; App-enabled authenticated LoginPage renders optional card; connected status navigates; skip is always offered | Connected/disconnected login, card start failure and stored-grant callback failure frontend checks pass; complete browser OAuth round trip unexecuted |
| Same block: Account owner and one shared grant | UserSettingsPanel and central page reuse card/hook; existing status/start/disconnect APIs reused; queries keyed by G5 user ID | Connect and disconnect component tests pass; reconnect button uses same grant replacement flow by inspection; no multi-user live UI test |
| Same block: destination and deployment gating | `githubConnectionReturnPath` rejects external/protocol-relative/backslash/control/recursive destinations; page and Account gate by instance App feature | Path tests pass; login preserve/skip and disabled existing login behavior covered; central page full navigation not directly tested |
| Same block: PR and comment recovery | Both PR dialogs render same account link on stable marker; composer navigates to shared page and retains existing local draft storage | Composer navigation test passes; both PR recovery-link render paths are source-inspected only |
| `g5.security.login` | Identity OAuth and App grant remain separate; no automatic grant inferred from sign-in | No violation found; platform OAuth unchanged |
| External comment attribution/drafts (`external_issues.g5.md`, retained comments design) | Only connection-navigation ownership changes; author, preview, submission, local draft and server posting logic retained | Composer's 8 tests pass; no alternate comment grant or bot fallback added |
| `g5.quality.review.cases`, `.evidence.claims` | New tests cover user selection, refresh/probe refusal, personal fallback, provider headers/reuse and central connection basics | Useful boundary evidence, but final destination binding is untested/incorrect; full public create route and real provider authorship remain unverified |

## Missing evidence / documentation follow-up (not additional code defects)

- The saved plan's actual ontology diff matches both changed blocks and their metadata. Its initial summary names only governance before later explicitly adding settings, a minor editorial point.
- Changed ontology footers still say implementation is unassessed/being implemented. Main reviewer should replace these with truthful scoped implementation/evidence and the unresolved binding issue after consolidation. Relevant governing assessments should be updated selectively where this review changes their known facts.
- No real GitHub PR was created, OAuth browser round trip observed, or live revocation exercised. Mocked token headers prove credential selection, not provider UI authorship in deployment.
- The new provider lifecycle integration invokes the service directly with seeded persisted state; it does not establish public route readiness/admission or missing-grant persisted error behavior end to end. Existing relevant route tests were inspected but not broadly rerun.
- No other concrete code defect found after reviewing the complete changed scope. Original source remains frozen and untouched.

## Completion addendum

Credential integration tests finished: `pr_creation_` passed all 3 tests in 122.66s (refresh/revocation/access checks; initiating actor versus another grant/personal fallback and revoked membership; non-App personal authentication). This closes the pending focused credential-runtime items above. Findings and the distinction between service/mock evidence and public-route/live-provider evidence are unchanged. Total focused runtime result: 46 frontend tests and 4 backend integration tests passed. No broad rebuild was performed.
