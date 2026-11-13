# PR authorship review

## Initial design before central-connection amendment

<details><summary>Saved implementation handoff</summary>

# GitHub PR authorship

## Ontology changes and governance

**Ontology changes:** Compared with branch base `main`, add
`g5.governance.publish.github-author` and its metadata to
[governance](../../ontology/features/governance.g5.md#attribute-github-pr-creation-to-the-initiating-user).

```diff
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 1ab30406..ec2a1a09 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -138,6 +138,22 @@ This follows the [publication contract](#publish-existing-work-under-the-actors-
 
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
@@ -260,6 +276,12 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
 - From [C048/C140](../../uplift/g5-lean/candidates/contracts.md).
 - ~80% · publication credential resolution and provider adapter; external provider permission matrix unexecuted.
 
+### g5.governance.publish.github-author
+
+- human-verified
+- [PR authorship request](../../chat_intent/codex-pr-user-authorship/request.md); user requested implementation of creation under their connected GitHub identity. Failure and compatibility rules are agent design decisions.
+- Unassessed · implementation and credential reuse will be inspected during coding.
+
 ### g5.governance.publish.proof
 
 - agent
```

**Relevant and governing ontology:**

| Commitment | Obligation and owner | Allowed case / counterexample | Conclusion |
|---|---|---|---|
| [g5.governance.publish.github-author](../../ontology/features/governance.g5.md#attribute-github-pr-creation-to-the-initiating-user) | Credential owner selects initiating user's App grant for new PRs, enforcing binding coverage and permissions | Authorized connected user / no bot or teammate fallback on missing grant | New explicit creation operation; preserve existing PR author |
| [g5.governance.publish.actor](../../ontology/features/governance.g5.md#publish-under-the-acting-humans-provider-authority) | Provider enforces user access; credentials resolved server-side | User has access / installation access alone cannot authorize user PR creation | Reuse user grant owner; inspect its refresh/failure policy in coding |
| [g5.governance.publish, g5.governance.publish.readiness](../../ontology/features/governance.g5.md#publish-existing-work-under-the-actors-provider-authority) | Publication owner operates on ready committed branch state | Ready source / dirty state bypass | Preserve retained readiness and push sequence |
| [g5.security.admission, g5.security.tenant, g5.security.tenant.write](../../ontology/concepts/security.g5.md#admit-actions-against-current-authority) | Routes authorize action; resolver checks actor membership and repository identity | Current actor and covered repo / cross-tenant or uncovered repo | Preserve all checks before using grant |
| [g5.security.credentials.publication](../../ontology/concepts/security.g5.md#keep-provider-publication-credentials-server-side) | Credential owner keeps tokens out of guest and public output | Server GitHub request / leaked token in error | Reuse encrypted grant and refresh mechanism |
| [g5.architecture.governance_owner](../../ontology/software_architecture/git_governance.g5.md#publication-and-approval-coordination) | PR service coordinates provider operations without owning token storage | Request creation credentials / caller assembling grant SQL | Extend existing credential API |
| [g5.design.minimal-mechanism, g5.design.change-locality](../../ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | Reuse existing App grant infrastructure and connection UI | Scoped PR change / rewrite push and merge credentials | No schema or unrelated policy change planned |
| [g5.quality.review.cases, g5.quality.evidence.claims](../../ontology/concepts/coding_principles.g5.md#review-both-permitted-and-forbidden-behavior) | Test owner verifies authority boundary and reports exact evidence | Connected actor, missing/revoked grant, binding refusal, non-App compatibility / happy-path-only test | Boundary tests plus source review |

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
PR creation. No material intent question remains. Source inspection must verify actual
grant refresh behavior, PR entry paths, binding validation reuse and existing connection
UI accessibility before coding depends on those operations. Source review and boundary
tests will be distinguished from live GitHub verification.

</details>

## Consolidated initial findings before repair

Three fresh full-scope reviewers independently inspected the frozen working tree. Their
raw reports are preserved in reviews/independent-{one,two,three}.md. Main-session source
review and lint are separate self-review evidence. No findings were supplied to reviewers.

1. **P1 — Actual PR destination was not bound to validated repository.** Reviewers one
   and three and main-session inspection traced separate git_url/provider_full_name
   inputs through import and creation. User tokens are not narrowed like installation
   tokens. The supplied happy-path protocol fixture itself permitted a mismatch. Accept:
   derive request destination from the validated Git URL, probe that destination with
   the user's token and verify GitHub's numeric ID against selected coverage. Cover stale
   display names and a name now resolving to a different ID; preserve matching identities.
2. **P2 — Login lost a connection page's nested return destination.** Reviewers one
   and two reproduced/explained the saved /account/github?returnTo=... route being sanitized
   to /. Accept: resume an explicitly requested connection-management route after login,
   bypassing the redundant post-login offer. Keep local-path/recursion protections and
   test connected and disconnected grants through the authenticated route.
3. **Main-session lint:** no-control-regex rejects the newly added control-character
   matcher. Replace it with explicit character-code validation, preserving rejection.

No additional concrete findings were reported. Minority/no-finding judgments do not
negate the directly demonstrated defects. Reviewer evidence is scoped to their report
commands; repeated runs are not additional unique coverage. Main tests before repair:
15 App binding tests + 22 PR lifecycle tests passed, 93 frontend tests passed followed by
25 App tests (including two additional login cases), and frontend typecheck passed.
Live GitHub and an external browser OAuth round trip remain unexecuted. Public PR route
regressions run in the 22-test suite; new authored-token behavior is tested at its lifecycle
owner with real restricted-role Postgres and mocked provider protocol.


## Repairs and verification

Main-agent repairs implement the accepted findings without changing their governing
contracts. Raw independent reports and the initial findings above remain unchanged.

- **P1 closed in scoped runtime evidence:** provider candidates now derive the actual API
  destination from the checked Git URL, and the user-token probe compares the returned
  GitHub ID with binding coverage. The integration fixture now uses consistent identities.
  The lifecycle test first persists `action_required` without a user grant and proves no
  create request occurred; it then connects the actor, supplies a conflicting stored
  provider display name and verifies successful creation at the bound destination with
  the actor token. Deleting the grant afterward still reuses the existing PR with no
  second POST. The credential test refuses a successful probe with a different repository
  ID, as well as 401/403/404/503 responses, while preserving successful refresh/access.
- **P2 closed in scoped runtime evidence:** authenticated login resumes a requested
  `/account/github?returnTo=...` page directly. Two App tests verify both connected and
  disconnected grants retain that page and its workspace/query return link. Plain login
  still offers connection only to disconnected users; the callback/failure and safe-path
  tests retain their earlier coverage.
- **Lint finding closed:** explicit character-code validation preserves control-character
  refusal without a prohibited control-character regex. Targeted ESLint exits zero;
  the two existing App warnings are unrelated to this change.
- **Repair-time cleanup:** removing the unused provider display-name projection eliminates
  the compiler warning introduced by destination canonicalization. This does not change
  the stored repository schema or the existing provider merge record policy.

After the substantive repairs, `cargo test -p g5-api --test api_github_app_bindings
--test api_workspace_pull_requests -- --test-threads=1` passed **15 + 22 tests** using
isolated local restricted-role Postgres and mocked GitHub, with Git transport redirected
only inside disposable subprocess fixtures. The seven targeted Vitest files passed
**99 tests**. `tsc --project tsconfig.build.json --noEmit`, targeted ESLint and
`git diff --check` passed. Targeted Rust/TypeScript formatting was applied. After the unused SQL projection cleanup,
the focused authored-PR lifecycle test passed again on a clean compilation (no warnings).
Ontology block/footer pairing, the pinned exact diff and Markdown file links also pass. Test-harness
canvas/React act warnings do not establish browser rendering coverage.

### Commitment coverage after repairs

| Commitment | Actual path | Discriminating check and result / gap |
|---|---|---|
| `g5.governance.publish.github-author` | `RepositoryCredentials::resolve(CreatePullRequest)` → existing grant owner → PR lifecycle POST | Actor grant only, no teammate/personal fallback, refresh, revoked/inaccessible access, missing-grant persisted failure, success and no duplicate on reuse pass. Stable repository ID mismatch and conflicting display name are covered. Live GitHub author attribution unexecuted. |
| `g5.governance.publish.actor` | Server credential resolver and provider adapter | Personal SCM compatibility passes; only creation switches to user token. Installation discovery/push and existing-PR reuse remain exercised. Broader external provider matrix unexecuted. |
| `g5.governance.publish` | Existing public create route and lifecycle | Full 22-test lifecycle suite passes, including branch publication failure retaining work and retry. Complete route/lease recovery audit outside this change. |
| `g5.governance.publish.readiness` | Existing save/preflight/create path | Public tests refuse failed saves, dirty stopped snapshots and replaced incarnation; preserve saved committed work on push failure. No readiness guard changed; full P6 audit remains open. |
| `g5.settings.github-connection` | Shared user-keyed hook/card, account page/panel, LoginPage, PR dialogs and comment composer | 99 scoped UI/helper tests include optional connected/disconnected login, nested destination, disconnect, initiation/callback failure, unsafe paths, disabled deployment behavior and central PR/comment recovery. External browser OAuth unexecuted. |
| `g5.security.login` | Existing identity OAuth callback → `/login`; separate App user connection endpoints | Source inspection preserves identity provider and session behavior; App route tests exercise post-login branching and safe return. No automatic grant inferred from platform login. Complete browser flow unexecuted. |
| `g5.security.admission` | Existing route authorization plus fresh credential membership/URL checks | Membership deletion, App binding denial and URL/coverage controls pass in resolver/binding tests. No new admission bypass; exhaustive concurrent revocation audit not claimed. |
| `g5.security.tenant` | Resolver's org-scoped repository/binding reads and per-user grant lookup | Cross-org binding tests and no teammate-token fallback pass under restricted runtime DB roles. No new tenant data store. |
| `g5.security.tenant.write` | Existing org-scoped lifecycle status/event writes and per-user grant owner | Missing authorization persists only the selected PR's action-required state; grant disconnect and normal lifecycle regressions pass. Alternate global writers remain outside scope. |
| `g5.security.credentials.publication` | Encrypted grant owner, App HTTP client, server-only PR adapter | Debug/error redaction assertions pass; probe does not echo provider response secrets. Frontend receives connection status and redirect URLs only. Exhaustive token propagation audit remains unassessed. |
| `g5.architecture.governance_owner` | Lifecycle coordinates; credential service selects; existing grant owner stores/refreshes | Source review confirms no token SQL in production PR coordinator and no additional ownership/store. Broader legacy governance gaps unchanged. |
| `g5.design.minimal-mechanism` | One additional credential operation plus reused grant API and shared card | Source review confirms no schema, App installation or sign-in migration. Removed feature-specific connection mutation and unused candidate projection. |
| `g5.design.change-locality` | Provider access probe in App client; selection in credential owner; shared frontend hook | Actual create destination now matches the credential policy's identity. Unrelated push/merge policies remain in existing owners. |
| `g5.quality.review.cases` | Three fresh full-scope independent reviews, consolidation and coherent repair | Raw reports preserved before fixes; allowed and forbidden branches tested at credential/lifecycle/UI consumers. No unresolved scoped findings. |
| `g5.quality.evidence.claims` | Commands and results above | Source inspection, mocked protocol plus real DB, and UI tests explicitly distinguished from live GitHub/browser OAuth. Unique test counts exclude reviewer and repeated runs. |

Changed feature metadata now names implementation and verification limits. Unchanged
security/architecture/design/quality footer assessments still describe wider unassessed
areas accurately and are retained. Publication actor/readiness assessments were updated
for the newly inspected paths without claiming completion of their wider contracts.
Review mode was three independent discovery agents plus main-session consolidation and
repair; no comparative cost/benefit measurement was performed.


## Follow-up: Make Remote repair history

The user reported a failed explicit push followed by a successful background push for
`bvandiver-g5/git-test` in workspace `test1`. Read-only inspection of local dev
`remote_sync_runs`, `remote_integrations`, `job_merge_runs`, and indexer logs confirms:
repair completed at 2026-09-30 02:31:54 UTC; explicit push failed with GitHub's
repository-not-found response at 02:31:59; background publication succeeded at 02:32:15.
Subsequent provider fetches report matching local/remote commit `4799e3522cf4` and no pause.
Both explicit WriteGit and automatic AutoPush select the same contents-write App
capability and push transport; the new PR user-token operation is not involved.
The provider rejection's underlying cause is not established by retained logs, and
no speculative credential/retry policy change was made.

A separate concrete display defect mapped `dirty_skipped`/`merge_active` to uncommitted
work even though the recorded dirty count was zero. `BranchTrackingPopover` now labels
that case as reconciliation in progress and explains that the active merge/repair must
finish, preserving the historical failed push. This is a bounded display correction
within existing intent; no ontology contract changed. Existing popover tests (22),
Prettier, targeted ESLint and whitespace checks passed. The dev frontend was rebuilt
to expose the correction; runtime history was inspected without modifying repository
content or clearing audit records.

## CLI retrieval: consolidated discovery before repair

Incremental scope: uncommitted retrieval amendment over checkpoint `75b522e7`, governed
by `g5.governance.pr.read` and the saved plan. Three fresh full-scope reviewers ran in
parallel without author history or peer findings; production/test/ontology edits were
frozen. Their independent reports are [one](reviews/pr-read-one.md),
[two](reviews/pr-read-two.md) and [three](reviews/pr-read-three.md). Reviewer one reused
the existing black-box CLI binary (3 tests passed); two and three used source checks.
No reviewer built Cargo targets. No measured comparison of review benefit is claimed.

Consolidated causes, saved before repairs:

1. **P1, all reviewers and parent runtime:** the internal info route lacks exact-method
   registration in `is_workspace_incarnation_internal_route`. A valid VM token receives
   401 before the handler; the real API test reproduced this. Add exact GET/UUID admission,
   preserve denial for other methods and token classes, and rerun valid/stale/foreign
   workspace cases through the full middleware.
2. **P2, reviewer two:** the separate handler-only `pull_request_reads.rs` fails the existing
   route scope source guard. Registrations have correct scopes, so this is CI integration,
   not a public authorization bypass. Move adapters into their existing registered route
   owners and share the read helper there; leave the guard intact.
3. **P2, reviewer two:** explicit `--workspace PATH` loses to ambient stable/environment
   workspace identity. Honor explicit checkout selection and fail on missing context;
   a scoped VM credential must fail on a foreign selected workspace rather than silently
   reading its current workspace. Verify real CLI requests for explicit/config/stable paths.
4. **Documentation follow-through, parent:** add the new callable reader to the module API
   table, and replace the new ontology block's planned assessment with measured evidence.

Pre-repair evidence: `cargo check -p g5d-cli -p g5-api-routes` passed; CLI unit tests 3/3,
black-box CLI tests 3/3 and provider-selector unit test 1/1 passed. API tests 3/4 passed:
App read-only credentials, personal reader token, URL/ambiguity/coverage/error/redirect
boundaries passed; incarnation success failed with 401. Two initial fixture failures
(role `primary`, visibility `private`) were corrected to established schema values;
no behavior expectation was weakened. `cargo fmt --all --check` and whitespace passed.
Live GitHub and installed guest/dev-server deployment have not been exercised.

## CLI retrieval: repairs and final coverage

All three consolidated code findings are repaired. Exact GET/UUID middleware admission
now includes the new route; negative method/path cases retain the typed-token boundary.
Handlers moved into `workspaces.rs` and `internal.rs` beside their registrations, sharing
`read_pull_request_info`; the handler-only file was removed and the route guard is unchanged.
Explicit local workspace selection now chooses the supplied checkout's config/metadata
rather than an ambient identity. Two new black-box cases verify classic and stable-context
selection; a mismatched VM token targets the selected workspace and is refused.

| Commitment | Actual path and discriminating evidence | Outcome / limit |
|---|---|---|
| `g5.governance.pr.read` defaults, presentation, selection | CLI parse/selection/render units; black-box environment, cwd config, stable metadata, explicit UUID and path; empty list and missing PR | Passed; readable output labels stored state and JSON distinguishes `g5`/`github` |
| `g5.governance.pr.read` provider boundary and no mutation | Real API + GitHub mocks: number/URL, ambiguity, unattached/uncovered repo, malicious URL, redirect, provider error, comments truncation; workspace row unchanged and no PR inserted | Passed; read list source contains no reconciliation, Git writes, merge or compute start |
| `g5.security.read`, `read.durable`, `tenant`, `tenant.rls` | Restricted-role public routes accept org-visible nonowner reader, reject invite-only outsider and foreign user; stored incoming/outgoing PRs | Passed; no broader authorization audit implied |
| `g5.security.admission`, `credentials.platform-token` | Full application middleware plus handler: valid VM stored and live GitHub reads, foreign workspace, ordinary-user token on internal path, replaced epoch, expired lease | Passed; current owner/VM checks also inspected in reused admission helper |
| `g5.security.credentials.publication`, redaction | Repository URL parsed by the existing strict credential owner; App read-only repository-scoped token and nonowner's personal token; unknown metadata preserved; error body and redirect withheld | Passed with mocked provider; real GitHub permission/rate-limit matrix unexecuted |
| `g5.architecture.governance_owner`, minimal mechanism/change locality | Existing durable list reused; separate bounded provider GET; adapters remain at route registrations; module API table updated | Preserved; no schema, background sync or new token store |
| Existing publication/authorship/readiness | `ReadPullRequests` remains distinct from new PR creation and publication; reused resolver inspected | Preserved by source inspection; no publication behavior changed |
| `g5.quality.review.cases`, `evidence.claims` | Three independent saved discovery reports, parent consolidation before repairs, real API failures caught missing middleware | Findings closed by route/CLI evidence; live GitHub and deployment not claimed |

API integration tests: 4 passed (including several positive/refusal cases per test).
Compiled g5d CLI tests: 5 passed. CLI units: 3 passed. Provider-selector unit: 1 passed.
Route structural guards: 6 passed, including scope enforcement and handler auth.
Incarnation route allowlist unit: 1 passed. Total targeted tests: 20 passed.
`cargo fmt --all --check` and `git diff --check` passed.
Documentation: `docs/g5d-pr.md` and both bundled agent workflow-recovery references.
The running dev API and guest runtime were not rebuilt for this amendment; local compiled
binary and test server were verified. No PR provider mutation or real GitHub request ran.

## JSON compatibility follow-up: original handoff

Before this amendment all PR `--json` output used a boolean flag and a source/pullRequest(s)
wrapper. The user requested JSON compatibility after the explicit `gh` comparison. The
saved design now requires field selection and unwrapped view/list output while preserving
other CLI commands. Previous 20-test read/authority evidence remains valid unless affected;
this follow-up changes CLI presentation and parser composition, not server admission.

## JSON compatibility: implementation snapshot and initial checks

Discovery is frozen at `/tmp/g5-pr-json-review-snapshot.tgz`. Three fresh full-scope
reviewers run in parallel with the same request/design and no implementation history.
Reports are `reviews/pr-json-{one,two,three}.md`; consolidation follows all reports.
No implementation repairs are made during independent discovery.

Initial executable checks: all 280 `g5d-cli` library tests passed serially; all seven
compiled `g5d --test pr_commands` cases passed; both existing `cli_commands test_cli_json`
success/error tests passed. `cargo fmt --all --check` and `git diff --check` passed.
Logs: `/tmp/g5-pr-json-{cli-all,blackbox,legacy,fmt}.log`. An earlier broad substring-filter
run in the sandbox hit two mock-server bind refusals and an environment-sensitive test;
the complete serial run with local socket access passed all three and the rest of the suite.

This follow-up does not alter the server read API, authorization or provider token paths.
Earlier server integration evidence remains applicable. Actual GitHub requests and the
running development/guest runtime are not part of these local test claims.

## JSON compatibility: consolidated discovery before repair

All three initial reports are saved and retained. Reviews one and two found no actionable
runtime defects; review three found F1: explicit empty `--json=` / `--json ""` incorrectly
succeed with non-JSON help. The parent accepts F1: the saved design distinguishes bare field
discovery from invalid empty field names, and the reviewer reproduced the empty forms in
both g5d and locally installed gh. The new documentation sentence permitting empty-value
help describes a contract violation; it does not supersede the governing design. Repair
must distinguish omitted optional values from supplied empty strings and test both forms
through the compiled command. No other findings require repair. No majority/vote inference
is used to dismiss the minority finding.

Coverage: all three independently inspected selection/shapes, no-context discovery,
mergeability and unknown/multiple-provider facts, comment demand, non-PR flag compatibility,
read/provenance/security interactions and minimal mechanisms. Prior API/security evidence
is retained. The reviewers' mock/subprocess evidence does not establish live GitHub or
deployed guest behavior. All three also identified final metadata reconciliation as
pending completion work. The incidental environment-sensitive unit failure in review
three was already covered by the parent's complete serial 280-test passing run.

Review effort: three fresh parallel contexts; no reliable per-review token or elapsed-cost
measurement was captured. Main-agent consolidation accepted one coherent bounded repair.

## JSON compatibility final assessment

F1 repaired: the PR option now preserves absent flag, omitted optional value and explicit
value as distinct states. Bare/root boolean JSON still lists fields; empty supplied values
fail validation before discovery/auth/network with empty stdout. The compiled no-context
regression tests explicitly exercise `--json=`, `--json ""`, invalid comma entries and bare
help against deliberately corrupt metadata. Documentation now matches the design. No
other implementation findings or consequential design amendments remain in this scope.

| Commitment | Actual path → counterexample/check → result |
|---|---|
| `g5.governance.pr.json`: syntax, shapes, compatible facts | `PrArgs`/`Selection` → split/equals syntax, plain object/arrays, field filtering, mergeability enums, missing/multiple-provider aliases → passed unit and compiled-command checks |
| Same: discovery/validation before I/O | Early `dispatch`/`pr::execute` → corrupt metadata and missing credentials with bare/empty/unknown fields → field help succeeds only for omission; invalid values fail with empty stdout |
| Same: non-PR compatibility | Scoped command factory and process error formatting → root/parent/leaf flags, positionals, existing JSON success/error and full CLI suite → passed |
| `g5.governance.pr.read` | Unchanged readers plus comment-demand query → current/explicit workspace and source, first-page comments/has-more, denial on invalid identity → seven compiled tests passed; earlier API tests retained |
| `g5.security.read`, `read.durable`, `admission`, `tenant`, `credentials.publication` | Same authorized user/incarnation clients and server owner → no new provider URL/token or mutation path → preserved by dependency inspection and previous API evidence; not a fresh live-provider audit |
| `g5.architecture.governance_owner`, `g5.design.minimal-mechanism`, `change-locality` | CLI-only projection and scoped flag construction → no server contract, schema, credential or compute change → preserved by three reviews and consolidation |
| `g5.quality.review.cases`, `evidence.claims`, `evidence.unknown` | Frozen independent reports plus coherent F1 repair → permitted and forbidden variants exercised, limits separated → complete for this increment |

Final checks after repair: **280 CLI unit tests + 52 existing compiled CLI integration
tests + 7 PR integration tests = 339 passed**. Logs:
`/tmp/g5-pr-json-final-cli.log` and `/tmp/g5-pr-json-final-integration.log`.
The focused five-test repair run also passed. Formatting and whitespace checks passed.

Scoped metadata now records the JSON implementation and evidence; unchanged governing
principle/security assessments remain accurate within their existing broader limits.
The saved ontology diff is refreshed. No live GitHub calls or running dev/guest deployment
were performed. Review and repair time/token costs were not measured separately; no cost
or improvement claim is made from using three reviewers.

## Shared PR writes: independent discovery snapshot

The user requested VM CLI create/update with exactly the UI's operation. The existing
saved design now includes that amendment and its source-inspection clarifications. The
implementation snapshot is `/tmp/g5-pr-write-review-snapshot.tgz`. Three fresh full-scope
reviewers receive the same request and design, without author history or peer findings;
raw reports are `reviews/pr-write-{one,two,three}.md`. Production, tests, and ontology are
frozen during discovery. Consolidation will precede any repairs.

Initial checks: API/routes and CLI compile; 280 CLI unit tests, ten compiled PR CLI tests,
six route guards, five VM PR integration tests and the review-workspace rejection test
pass. The provider-authorship integration recheck passes after fixing its local mirror's
missing named origin. The first full PR suite had two fixture failures (missing origin and
invalid transfer digest), both corrected; a final complete suite is pending. Both bundled
PR skills validate, and their scoped surface check passes. The root CLAUDE contract checker
reports eleven pre-existing missing clauses in the unchanged root instruction file.
No live GitHub or deployed worker test has run.

## Shared PR writes: consolidated discovery before repair

All three initial reports are saved at `reviews/pr-write-{one,two,three}.md` and retained.
Two reviewers received fresh contexts. The platform refused a third new agent because of
its thread limit, so the third review used the earlier independent JSON reviewer, which
had no implementation or review history for this write increment. All three received the
same full scope and no author explanation, known-gap hints, or peer findings. Thus this
was three independent reviews of writes, with only two wholly fresh task contexts.

Accepted findings, checked against the shared service and consumers:

1. **F1 / P1 — target authority after asynchronous preparation (review two).** Target
   readability was checked only in the initial transaction. Organization-level RLS does
   not enforce visibility/invitation changes. Recheck target access in every fresh phase
   before evidence or effects, for both adapters; preserve a currently invited reader and
   a read-only target. Test revocation while save/push is paused through real HTTP routes.
2. **F2 / P2 — outer provider failures hidden as success (reviews one and two).** The old
   UI log-and-return behavior can return an earlier `local_only` or `open` summary after
   provider admission, synchronization, summary or commit failure. Preserve the committed
   G5 PR, but return one actionable partial-failure error with its identity, failed phase
   and uncertain/not-attempted outcome to both consumers. Keep current VM fencing, and
   never expose raw internal error details or automatically replay the mutation. Test
   post-local-commit VM refusal and provider persistence failure, plus retry reuse.
3. **F3 / P3 — bundled Codex profile size (review three).** The new pointer grew the file
   from 763 to 769 lines against its existing 766-line bound. Shorten the pointer without
   raising limits; verify the exact size predicate and scoped profile checks.

Minority findings are retained based on source evidence, not vote counts. Review one's
body-update clarity note is also accepted: explain that the stored G5 description affects
new provider PR creation and does not edit an existing GitHub PR's title/body.

The reports cover shared ownership, caller/tenant/source/target authority, save/push
ordering, publication credentials, provider outcomes, CLI JSON/transport, skills and
packaging. Tests use real HTTP routes and restricted-role databases with scripted native
peers and mocked providers. Actual active-agent → native Git → API callback execution,
live GitHub/OAuth and a rebuilt/deployed worker remain unverified; source inspection alone
is not evidence of those outcomes. No product-intent amendment is needed for these repairs.
Review and repair time/token costs were not measured separately.

## Shared PR writes: repairs and final assessment

**F1 closed.** `PullRequestCaller::authorized_db` opens fresh typed source authority,
rechecks shared source restrictions/current ownership and target readability in every
phase. The public and internal HTTP regression cases pause native preparation, save a
new source commit, revoke target visibility through its real owner API, then require
403 with no PR/evidence record and the saved commit retained. Adding a current viewer
invitation through the member API permits creation against the private, read-only target.

**F2 closed.** Admission, synchronization, summary and commit failures after the local
commit now return the existing conflict-with-details envelope with the saved PR UUID,
phase and `not_attempted`/`unknown` provider outcome. Only a safe error code is exposed;
raw internals stay in server logs. Ordinary per-repository provider failures retain their
recorded successful-G5-result semantics. A queued incarnation replacement after the local
PR commit produces the explicit admission error with the persisted ID, and an authorized
retry reuses it. A deferred provider constraint failure after mocked GitHub acceptance
produces an unknown commit result, preserves the previous durable provider record, hides
injected internal text and allows a public retry to adopt the same remote PR without a
second create. Compiled CLI checks show human and selected-JSON commands fail with empty
stdout and the saved ID/recovery instruction, with no replay. The existing UI error path
renders that message; its hook now refreshes both PR lists on settlement so a saved PR is
not hidden by a failed provider phase. A hook regression exercises the actual API error
adapter and both cache invalidations.

Later discoveries during coherent repair: source owner/mutability checks also belonged
in every fresh admission, including the human provider phase; they are now centralized
with target checks. The UI's old success-only PR-list invalidation was incompatible with
explicit partial errors and was repaired as described above. These refine the existing
admission/recovery commitments; no governing meaning was weakened. Initial repair runs
exposed a test-only two-connection-pool starvation and a mocked error envelope missing its
required request ID; both fixtures were corrected, and the exact failed checks now pass.

**F3 closed.** The Codex profile pointer is concise and retains the skill routing and
save/push warning. Current profiles measure 765 lines/63,521 bytes (Codex limits
766/64,659) and 808 lines/66,765 bytes (Claude limits 808/71,261). Skill copies are identical,
both validate, required manifests include them, the Claude tracked-file allowlist includes
it and scoped surface checks pass. Docs now explicitly distinguish G5 description updates
from existing GitHub PR title/body editing.

| Commitment | Actual path → counterexample/check → result or gap |
|---|---|
| `g5.governance.pr.write`, `g5.architecture.pr_flow` | Thin public/internal adapters → one `create_or_update` owner; VM create + UI retry reuse one persisted PR → passed; `update` is the same CLI variant |
| `g5.security.owner`, `admission`, `tenant`, `tenant.write`, `login.service-identities`, `g5.runtime.lease` | Typed caller/actor scope + runtime owner/epoch/lease fences → wrong token/source, stopped/replaced/expired/transferred callers and post-save replacement → denied without later effects; all fresh phases re-admit source and target |
| `g5.security.read`, `g5.governance.pr.write` target authority | `authorized_db` and actual visibility/member routes → revoke during native preparation, preserve completed source commit, current invitation/read-only target succeeds → F1 regression passed |
| `g5.governance.publish`, `publish.readiness` | Shared native commit/push before evidence → failed save/push, stopped dirty snapshots, retained work and reuse → API tests passed; scripted peers do not prove actual guest native execution |
| `g5.governance.publish.actor`, `publish.github-author`, `g5.security.credentials.publication` | Existing credential/provider owners → internal actor grant, no grant fallback, public existing-remote reuse and provider commit failure recovery → mocked provider checks passed; live provider matrix remains unexecuted |
| `g5.governance.pr.write` partial results/recovery | Shared post-local error boundary + standard CLI/UI consumers → VM phase refusal and GitHub-accepted/provider-commit failure → saved identity and uncertain outcome visible, no blind retry, PR lists refreshed; F2 closed |
| `g5.governance.pr.json`, `pr.read` | Shared selector/projection and transport → bare/invalid fields before I/O, body/file/stdin, one object, existing reads and no ambiguous-response replay → all ten compiled CLI cases passed; previous read API evidence remains valid |
| Review/source restriction | Shared source admission and dedicated review handback → review VM/public rejection before RPC, read-only source rejected and read-only target allowed → passed |
| `g5.architecture.governance_owner`, `g5.design.minimal-mechanism`, `change-locality` | Extracted service reuses native, publication, provider, credential and access owners → no schema, token store or parallel PR algorithm → independent inspection and repair preserve scope |
| `g5.quality.review.cases`, `evidence.claims`, `evidence.unknown` | Three initial reports + parent consolidation and coherent repairs → minority findings retained and discriminating races executed → scoped review complete, explicit limits below |

Final executable evidence: 31 PR API integration tests, one review-workspace integration
case, ten compiled PR CLI tests and 50 UI tests pass after repair. Earlier unchanged CLI
unit (280), route guard (6) and incarnation-route unit (1) evidence remains applicable:
**379 passing cases across those suites**. Final API/CLI/UI logs:
`/tmp/g5-pr-write-repaired-{api-final,review-workspace,cli-final}.log` and
`/tmp/g5-pr-write-ui-tests-final.log`. The UI suite emits existing act warnings; assertions
pass. Reviewer staging tests (3) and PR unit tests also pass but are not double-counted.

Both new skills pass validation and scoped surface checks; current profile size bounds
pass. Broader profile checks are not clean: the unchanged root CLAUDE file lacks legacy
required clauses, and the unchanged node-explainer copies differ (all verified identical
to HEAD individually). These unrelated baseline issues remain outside this repair.
No actual active-agent/native Git callback cycle, live GitHub/OAuth verification or running
API/worker deployment was performed. The remaining gap is environment execution, not an
unresolved reviewed code defect. All three initial findings are repaired; no new PR,
commit or dev-stack restart was created by this follow-up.

Final static checks: `cargo fmt --all --check`, `git diff --check`, frontend build
TypeScript checking and scoped ESLint pass. ESLint reports four existing unused-symbol
warnings in unchanged declarations. Logs: `/tmp/g5-pr-write-{fmt-final,ui-typecheck,ui-lint}.log`.
The saved ontology diff and changed metadata now reflect the shared implementation and
these precise verification limits; unrelated governing gaps remain unchanged.

## gh PR surface compatibility audit — 2026-09-30

Read-only interface audit requested by the user, separate from the preceding implementation
conformance review. Compared current G5 source and the compiled CLI against installed
`gh 2.92.0 (2026-04-28)`, its help and parser behavior, the current official CLI manual and
version-pinned upstream source. No production changes or provider mutations were made.
The current manual includes newer attachment flags absent from the installed gh version;
version-sensitive observations below name the installed baseline.

**Verdict: partial syntax/field compatibility, not command or API interchangeability.**
The earlier user selection deliberately made G5 workspace PRs the default and GitHub
number/URL lookup explicit. The new write requirement deliberately preserves UI save/push
behavior and one shared owner. Those remain valid G5 commitments; compatibility must not
silently replace them with a different publication flow.

| Surface | Alignment and difference |
|---|---|
| Command inventory | Four matching canonical names: `create`, `list`, `status`, `view`. G5 adds `update` as an alias of create and `show` as an alias of view. Fourteen gh commands are absent: `checkout`, `checks`, `close`, `comment`, `diff`, `edit`, `lock`, `merge`, `ready`, `reopen`, `revert`, `review`, `unlock`, `update-branch`. The `new` and `ls` aliases are absent. [gh command catalog](https://cli.github.com/manual/gh_pr). |
| View selection | Number and clean GitHub PR URL syntax overlap. G5 additionally accepts its UUID; it does not accept branch selectors. No selector chooses the latest outgoing G5 workspace PR, with closed/merged fallback, rather than the GitHub PR for the current Git branch. [gh view](https://cli.github.com/manual/gh_pr_view). |
| List | `-s/--state` values and `-L/--limit` default 30 match, but G5 lists stored incoming/outgoing workspace PRs, not the repository's live GitHub PRs. G5 lacks author/assignee/head/base/label/draft/App/search filters, browser output and `ls`; it caps limit at 1000. [gh list](https://cli.github.com/manual/gh_pr_list). |
| Status | G5 returns open workspace PRs as a flat JSON array. gh groups current-branch, authored and review-requested PRs; its JSON is an object with `currentBranch`, `createdBy`, `needsReview`. G5 lacks `--conflict-status` and provider CI/review summaries. [gh status source](https://github.com/cli/cli/blob/v2.92.0/pkg/cmd/pr/status/status.go#L155-L164). |
| Repository scope | gh inherits `-R/--repo [HOST/]OWNER/REPO` across commands. G5 exposes `-R` only on `view`, requires a GitHub selector, accepts `OWNER/REPO` only, restricts it to attached workspace repos and accepts github.com PR URLs only. `g5d pr -R acme/api view ...` fails; `g5d pr view 42 -R acme/api ...` is valid. G5 also requires workspace context or `--workspace-id`, with VM identity bound to its own source. |
| Create input | `-b/--body` and `-F/--body-file`, including stdin `-`, align. G5 requires a destination workspace UUID via `--target-workspace-id/--target`; this is not gh's `-B/--base` branch or default-base behavior. G5 has no `--title`, `--head`, `--draft`, fill variants, labels, reviewers, assignees, projects, milestone, template, editor/web, recover, dry-run or maintainer-edit switches. [gh create](https://cli.github.com/manual/gh_pr_create). |
| Create effects/output | G5 saves pending workspace edits, pushes, reuses/creates its G5 PR and coordinates potentially multiple provider PRs. gh's branch-based creation warns about uncommitted edits and can push/fork existing history. G5 prints a G5/provider summary; gh prints the new PR URL. G5 supports create JSON selection; gh 2.92 rejects `create --json`. [gh create source](https://github.com/cli/cli/blob/v2.92.0/pkg/cmd/pr/create/create.go#L746-L748). |
| Update versus edit | `g5d pr update` is the same save/push/upsert as create. It is neither `gh pr edit` (provider metadata editing) nor `gh pr update-branch` (bring base changes into the PR branch). `--body` changes G5's override and the body of a newly created provider PR; it does not edit an existing provider PR's title/body. [gh edit](https://cli.github.com/manual/gh_pr_edit), [gh update-branch](https://cli.github.com/manual/gh_pr_update-branch). |
| Formatting | Selected view JSON is an object; list JSON is an array; comma-separated field names and uppercase state/mergeability values align on supported facts. G5 lacks `--jq`, `--template`, `--web`; its inherited `-q` means quiet, not jq. Root `g5d --json` is a separate boolean interface. |
| Success/failure contract | G5 uses its own exit-code catalog (authentication/network 3; Git 4; not-found 6). gh documents authentication 4 and cancellation 2. G5 can exit 0 with a saved G5 PR and recorded provider failure; callers must inspect provider status before asserting GitHub creation. Outer incomplete provider phases return nonzero with the saved G5 ID. [gh exit codes](https://cli.github.com/manual/gh_help_exit-codes). |

### JSON field and type audit

Installed gh exposes 46 PR field names. G5 live GitHub view exposes 25 names: 20 shared
names plus `source`, `repository`, `commentCount`, `reviewCommentCount`, `commentsHasMore`.
The counts describe field-name overlap, not identical schemas. Missing gh fields include
`id`, `reviewDecision`, `statusCheckRollup`, `reviews`, `reviewRequests`, `labels`, `files`,
`commits`, `assignees`, `closed`, `closedAt`, `mergedBy`, `mergeCommit`, `autoMergeRequest`,
`headRepository`, `headRepositoryOwner`, `isCrossRepository`, `maintainerCanModify`,
`milestone`, `projectCards`, `projectItems`, `reactionGroups`, `latestReviews`,
`closingIssuesReferences`, `potentialMergeCommit` and `fullDatabaseId`.

Workspace output supports a different 29-field schema; only 13 names overlap gh. Its
`id` is a G5 UUID, `state` and timestamps describe the G5 record, and `body` is the stored
override rather than necessarily the provider body. Singular provider fields become null
when zero or multiple provider records exist. `author`, checks and review fields are not
accepted in default G5 view/list/status mode. Switching a selector from UUID/omitted to
number/URL therefore changes both the entity and the accepted field catalog.

Even in live mode, nested output is reduced: `author` has only login; discussion comments
use REST numeric IDs and a smaller object, with at most the first 100 plus the G5
`commentsHasMore` indicator. gh's provider IDs/schema come from its own API model.
[G5 projection](../../../backend/g5d-cli/src/pr_json.rs),
[G5 provider read projection](../../../backend/g5-api-core/src/services/workspace_pull_request_read.rs),
[gh PR model](https://github.com/cli/cli/blob/v2.92.0/api/queries_pr.go).

### Executed parser probes

Ran both executables from an isolated temporary directory with empty tool authentication
configuration and no provider requests. Source inspection covers actual request/effect
semantics; help/probes alone do not claim live-provider equivalence.

| Probe | gh 2.92 | g5d |
|---|---|---|
| `pr view --json` | Field list on stderr, exit 1 | Field list on stdout, exit 0 |
| `pr view --json=` | Passes parsing to authentication (exit 4 in this environment) | Explicit-empty validation error, exit 2 |
| `pr view 42 --json id` | Field accepted; reaches auth | Unsupported field, exit 2 |
| `pr view --json author` | Field accepted; reaches auth | Unsupported in default G5 mode, exit 2 |
| `pr -R acme/api view --json` | Repository flag accepted, then field help | `-R` parser rejection |
| `pr ls --json` | Alias accepted, then field help | Unknown subcommand |
| `pr create --json` | Unknown flag | Field discovery, no mutation |
| `pr view 42 --json number -q .number` | jq accepted; reaches auth | `-q` is quiet; expression rejected as an extra positional |

### Follow-up priorities

1. Treat existing compatibility as a documented subset. Preserve the chosen workspace
   default, typed VM restrictions, server credentials and single UI/CLI publication owner.
2. Highest script risks are differing entity selection/status shape, missing or differently
   typed JSON fields, and exit 0 with incomplete provider publication. Decide explicit
   compatibility policy for these before expanding same-named commands.
3. Small additive improvements could include `ls`/`new` aliases and more live-provider
   JSON fields. Repository-wide reads, branch selectors, jq/template, provider editing,
   metadata flags and gh status grouping need separate scope/behavior decisions.
4. Do not alias `edit` or `update-branch` to the existing G5 `update`: those commands promise
   different effects. Any supported publication flags must feed the shared UI/CLI owner.

No code, ontology or command behavior was changed by this audit. This appendix is the
saved evidence; broad feature parity is not implied by the earlier JSON-compatibility work.

## Target, title and node ID follow-up — discovery baseline

Scope: the user's “make these fixes” instruction implements implicit destinations,
`-t/--title` and GitHub node ID selection. Governing design is the existing
`ontology-plan.md`, especially its target/title amendment and first-section commitments.
The earlier audit above records the superseded required-target/no-title/missing-ID surface.
The pre-amendment complete plan is retained at `/tmp/g5-pr-before-target-title-plan.md`;
the review snapshot is `/tmp/g5-pr-followup-snapshot.tgz`.

Initial checks: API/routes and CLI compile, frontend type check and 69 UI tests pass;
12 compiled CLI tests and four live-route mocked GitHub read tests pass. The first full
API suite had 27 passing / five failing cases: two extra-call RPC permit shortages, two
fixtures that attempted to mutate immutable workspace kind, and one invalid working name.
Those fixture errors are corrected without changing behavior assertions; rerun pending.
The sandbox initially prohibited localhost binds/DB access; permitted local test execution
resolved that environment issue. No live GitHub or deployed worker execution is claimed.

The provider title operation follows [GitHub's update PR API](https://docs.github.com/en/rest/pulls/pulls#update-a-pull-request).
Three fresh full-scope discovery reviewers write `reviews/pr-followup-{one,two,three}.md`.
They receive request/design/code, not implementation-session history or each other's findings.
Production and ontology stay fixed during discovery; consolidate before repairs.

### Follow-up consolidated findings before repair

All three independent reports are saved in `reviews/pr-followup-{one,two,three}.md`.
Consolidation by source inspection, not vote count:

- F1 / P2 — two reviewers and self-review: failed title PATCH discards the found PR;
  generic upsert nulls its number/node ID/URL. Preserve the known pull on failure and test
  both discovery and persisted identities.
- F2 / P2 — all reviewers and self-review: the new target cache ignores existing workspace
  invalidations. Reviewer one reproduced this through actual target/delete hooks; reviewer
  two/three independently probed QueryClient invalidation. Put the query under the existing
  workspace invalidation family and test a mounted hook with an actual mutation.
- F3 / P2 — reviewer one found retry loss; reviewer three independently found overlapping
  request regression. One cause: transient title input outlives the local commit without a
  durable pending intent. Add a nullable pending title to the G5 PR; under the existing source
  fence the provider owner reads current pending intent and clears it only after successful
  synchronization. Test failed update/create recovery and an older continuation after newer
  accepted intent; ordinary successful omission must preserve provider titles.
- F4 / P2 — reviewer two finding, one evidence gap and self-review lead: the new title effect
  is incorrectly gated by new-PR creation eligibility. `Identical`/`SourceAlreadyMerged` can
  coexist with an existing recorded provider PR while other members differ. Sync metadata
  for recorded PRs without creating empty PRs; preserve readiness/authority and existing
  provider status. Validate with fresh HTTP preflight over actual Git member histories.

Baseline before these repairs: 32 PR API + four read API + 12 compiled CLI + eight focused
CLI unit + 69 UI tests pass. Wider route units: 240 passed initially and the sole localhost
restriction passed with permitted sockets; the two unrelated CLI host socket tests likewise
passed on rerun. Broad CLI substring run was 75 pass/two environment failures, not a full
CLI-suite claim. Formatter, whitespace, type check, scoped lint (warnings only), both skill
validators and skill surface sync pass. No live provider/deployed VM claim.

### Later repair discovery: public-user title serialization

Reviewer one rechecked the coherent repair and confirmed F1/F2/F4 and sequential F3
recovery, but found that the assumed source fence is only FOR SHARE for public users.
A's provider context can load pending A, B can accept/synchronize B, and A's in-flight
PATCH can finish last. The previous claim of sufficient source fencing was incorrect.
Retain F3 open until the provider owner locks the G5 PR row before loading current title
intent and keeps that lock through provider effects/status/clear. Local title UPDATE takes
the same row lock in the same source-before-PR order. Add overlapping public requests with
a delayed provider response and observe B waiting on its actual PR UPDATE, then assert
final G5/provider title agreement and no pending intent.

## Target, title and node ID final assessment

All four consolidated findings are repaired. The final incremental check in
`reviews/pr-followup-one.md` closes the remaining public-user concurrency finding;
it does not replace the three independent full-scope discovery reports.

| Finding | Repair and executed evidence |
|---|---|
| F1: failed title update loses provider identity | Failure records retain the discovered pull. API regression checks number/URL in the result and number/node ID/URL in storage, including first discovery. |
| F2: stale target choices | Query key joins the existing workspace invalidation family. Mounted target/delete hooks verify refetch without focus. |
| F3: lost or stale pending title | Migration 041 stores pending intent with the local title. Provider owner locks the PR row before loading intent and retains it through effects/result persistence. Failure/retry, newer accepted intent, omission after completion and two overlapping public requests pass. The concurrency regression observes the second request waiting on its actual title UPDATE while the first provider response is delayed. |
| F4: existing unchanged-member PR omitted | Existing recorded provider PRs receive title updates without new empty PR creation. Public API regression uses actual equal Git member heads alongside another changed member. |

| Commitment | Implementation and scoped assessment |
|---|---|
| `g5.governance.pr.target` | Shared `workspace_pull_request_create::targets` supplies UI choices and omitted-target resolution. Public/VM tests cover parent preference, Base fallback/order, visibility, no default, explicit override and access revocation without retargeting. |
| `g5.governance.pr.title` | CLI title transport enters the shared owner, coordinated publication persists title/pending intent, and provider lifecycle consumes it under the PR row lock. Blank input refuses effects; create/reuse/omission and provider recovery are covered. |
| `g5.governance.pr.json` | GitHub projection uses REST `node_id`; `pr_json` accepts `id` in provider mode. Numeric and URL selectors return node IDs, independently of numeric PR/database IDs. G5 UUID projection is unchanged. |
| `g5.governance.pr.write`, `g5.architecture.pr_flow` | Public and incarnation-authenticated VM adapters still enter the same complete save/push/create/update owner. The UI consumes its ordered target choices; no second publication sequence was introduced. Existing refusal, preparation and partial-result regressions pass. |

Final executed evidence:

- 33 PR write/API and four PR read/API tests pass after the PR row-lock repair
  (`/tmp/g5-pr-followup-api-complete.log`). The focused concurrency/retry test also
  passes separately (`/tmp/g5-pr-followup-concurrency.log`).
- 12 compiled PR CLI tests, three focused command units and five JSON units pass.
  No CLI code changed during the subsequent provider/cache repairs.
- 70 focused frontend tests pass, including mounted target-cache invalidation;
  frontend build type check passes. Scoped ESLint reports zero errors and 98 existing
  warnings. Both worker skill copies validate and their surface-sync check passes.
- Provider lifecycle units: 17 passed initially and the sole socket-restricted case
  passed with permitted local networking (18 total). Route units: 240 plus the sole
  socket-restricted case passed on permitted rerun (241 total). These are supporting
  pre-serialization checks; the final API suite compiles and exercises the row lock.
- Rust formatting, whitespace and migration checksum validation pass. Migration 041
  is applied by the isolated API test databases.

Evidence uses restricted-role test databases, real local Git histories, scripted native
peers and mocked GitHub HTTP. It does not establish live GitHub behavior, deployed VM
execution or active guest/native callback behavior. No dev stack restart, deployment,
commit or live provider mutation was performed for this follow-up. Broad `gh pr` parity
remains outside this three-fix scope; the compatibility audit retains the other differences.


## Automatic PR refresh after push — discovery baseline

Scope: user's approved change makes successful pushes update existing open PRs, preserving
approval versions and exact merge checks. The existing plan's automatic refresh amendment
and `g5.governance.pr.follow-push` govern. Earlier PR CLI/authorship/title work remains in
the same uncommitted checkout and is not the scope of a fresh broad audit. Its affected
shared consumers are in scope. The pre-amendment plan is saved at
`/tmp/g5-pr-before-auto-refresh-plan.md`.

Implementation baseline adds published-evidence capture, an existing-PR continuation,
provider observation without implicit push, durable pending/error fields (migration 042),
an event-driven/periodic reconciler, provider-merge serialization and client invalidation.
Discovery snapshot: `/tmp/g5-pr-auto-review-snapshot.tgz`. Three fresh independent reviewers
will save read-only reports in `reviews/pr-auto-{one,two,three}.md` before consolidation.

Executed initial native Git test passes: real smart-HTTP push updates one existing PR,
performs no extra VM save/push and permits merge without Update PR. Provider failure/retry,
remote publication wait, no-op, stale merge rejection, target access loss and terminal/
absent PR tests pass after correcting fixtures (visibility enum and merged provider reply).
The original WebSocket integration test uses a fake invalidation implementation; the new
regression instead exercises real WebSocketManager with QueryClient and passes. Its abandoned
fake-based failure is not production evidence. Additional published-snapshot/approval test
is running. No live GitHub, deployed VM or migration of a running stack is claimed.

### Automatic refresh discovery consolidation (before repairs)

All three independent frozen-snapshot reports are saved in `reviews/pr-auto-{one,two,three}.md`.
The author's separate discovery is preserved in `/tmp/g5-pr-auto-self-review.txt`.

| Finding | Sources and disposition |
| --- | --- |
| A1: evidence updates erase unfinished provider work | All three reviewers; accepted. Every changed-evidence writer must durably mark continuation; discovery must also detect older provider pins. Completion is separate from local preflight. |
| A2: unbounded Git observation starves later work | Reviewers two/three; accepted. Use bounded, cancellable provider reads for automatic remote-head observation, with no Git push. |
| A3: ready rows hide refresh failures | Reviewers one/two and author; accepted. Expose pending/error separately and render it independently of local mergeability. |
| A4: content-write policy blocks permitted push completion | Reviewer three, reviewer one's explicitly tentative policy concern, author; accepted after inspecting GitOperation policy. Automatic continuation keeps ownership/transfer admission but uses committed Git policy. Unchanged scans must remain no-ops. |
| A5: stale target access after PR-lock wait | Reviewer three; accepted. Lock target authority and recheck after waits before effects. |
| A6: included provider member keeps obsolete pins | Reviewer three and author; accepted. Reconcile an existing member independently of eligibility to create a new empty provider PR. |
| A7: PR-before-workspace lock upgrade deadlocks merge | Author; accepted. Metadata-only refresh must not upgrade workspace locks while holding PR; preserve workspace-before-PR order. |
| A8: recorded provider identity differs from current attachment | Author; accepted. Verify canonical identity before provider observation, as existing title synchronization does. |

Initial tests passed (45 backend API/native/read tests, four real WS authority tests, frontend
typecheck); they did not distinguish these counterexamples. No governing intent is weakened.
Repairs and focused evidence follow below; original reports remain unchanged.

### Later repair discovery: nested credential scopes exhaust the tenant pool

The actual overlapping automatic/manual Refresh test reached the PR wait but timed out.
Inspection found automatic provider synchronization held one tenant transaction while a
manual Refresh held the second waiting for the PR; credential resolution then requested a
third tenant connection. PostgreSQL cannot detect this pool/row-lock cycle. The credential
owner needs to accept an already-authorized caller scope for its tenant reads, retaining
all current membership, repository identity, binding, capability and credential selection
checks. Automatic provider operations reuse that scope rather than widening test pools.
This is an implementation repair under the existing progress/authority commitments.

### Automatic refresh final assessment

Three independent full-scope discovery reports were consolidated before one coherent repair.
Reviewer one performed bounded incremental checks of the repairs and reported no remaining
new material defect. Reports preserve initial findings, later discoveries and dispositions.
Review/repair token or time costs were not separately measured.

| Finding / commitment | Repair, authoritative consumer and executed evidence |
| --- | --- |
| A1: durable progress (`pr.follow-push`, recovery) | Shared insertion/evidence writes mark pending; completion is separate and discovery also compares provider source pins. Public local Refresh before the scan retains work; a simulated pre-migration missing marker is recovered from provider pins. Periodic scans recover missed events and failure/retry; initial explicit creation persists the obligation before its provider phase. |
| A2: bounded progress | Automatic source/target observation uses cancellable GitHub HTTP with ContentsRead credentials. A 90-second delayed response times out, the later organization's local PR advances within 40 seconds, and cancellation completes within two seconds. No abandoned blocking Git task or child process. |
| A3 / R3: visible truthful completion | Summary exposes pending/error; incoming list, source Create/Update dialog and Prepare to Ship show the warning independently of old local readiness. Real WebSocketManager/QueryClient invalidation and mounted render tests pass. CLI JSON accepts pending/error fields; worker guidance explains them. |
| A4: committed-work admission | Automatic continuation uses GitOperation with current owner/transfer checks. Real smart-HTTP pushes on read-only and freezing sources update one existing PR, make no extra guest RPC and merge without Update PR. An unchanged read-only scan is a no-op. |
| A5: current target access | Source/target identity SHARE locks precede PR; invitation SHARE lock and access recheck follow the PR wait. Revoking the invitation while the real automatic operation waits prevents evidence changes; restoring it allows recovery. The locked source's owner FK preserves organization membership, and credential resolution rechecks membership at provider admission without retaining another pooled transaction. |
| A6: included recorded members | Existing provider records reconcile even when no new empty PR is eligible. A real multi-member Git fixture advances an already-included member's pins, retains pending work for failures and creates no new empty provider PR. Exact provider/source and target checks remain at merge. |
| A7 / R2: lock order | Metadata-only path/target PR helpers lock the PR row, with actual merge application retaining its existing workspace fences. Actual public target Refresh waits on automatic PR refresh while holding its owner lock; both finish and subsequent merge accepts the refreshed exact SHA. |
| Later pool cycle | `RepositoryCredentials.resolve_in_scope` reuses the already-authorized tenant connection while retaining actor/org, membership, URL, binding, capability and credential policy checks. The overlap regression runs against the normal two-connection test pool. App-token scope regression verifies successful automatic refresh uses narrowed installation credentials; existing App authorship/read API tests pass. |
| A8: provider identity | Canonical current attachment must match the recorded provider repository before any provider HTTP. Deliberately mismatched fixture makes no HTTP request; recovery after restoring identity passes. |
| R4: accepted merge version (`merge`) | Every uncertain automatic provider branch rolls back evidence and pins. Deterministic external-merge race: first GET sees open A, source-ref observation merges A while internal source B is unpublished; immediate failure and subsequent terminal retry both retain A. Known completed provider PRs and closed/absent G5 PRs do not advance. |
| Published refs (`publish`) and approval version | Two repositories advance while a separate retained dirty snapshot exists. Automatic evidence uses published heads only; the stored approval vector is unchanged and its public approval state becomes stale. No save/start/approval writer is added. |
| Shared flow / minimality (`pr_flow`) | Explicit UI/CLI retains its complete save/push owner; the narrower automatic continuation calls the same evidence and provider owners. Two durable fields, the existing branch hint bus, startup/periodic inventory and one PR update event implement recovery without a second PR creation pipeline. |

Executed complete suite: 41 PR API + five native push + four read API + one populated
migration tests pass (`/tmp/g5-pr-auto-final-api.log`). Twenty coordinated publication units
and five CLI JSON units pass. All 49 focused UI tests pass; build typecheck passes. The final
automatic-only rollback/admission delta passes eight automatic API tests and the native
push test (`/tmp/g5-pr-auto-final-delta.log`); API binary check passes. The last source-dialog
copy change passes all 18 dialog tests and another build typecheck. Formatter, migration
checksums, scoped lint (zero errors, three existing warnings), both worker skill validators
and worker surface sync pass.

Earlier repair attempts exposed fixture omissions (membership foreign key and response
envelope), an unavailable tenant control-plane table, and the actual two-connection pool
cycle. They are not claimed as successful checks. `/tmp/g5-pr-auto-overlap2.log` records the
passing concurrency repair; `/tmp/g5-pr-auto-migration2.log` records the populated upgrade.

Remaining boundaries: live GitHub and deployed VM execution were not exercised; recovery is
tested by a no-event full pass rather than killing/restarting a process. The prior explicit
manual-only ability to refresh/replace evidence after provider acceptance is recorded by
reviewer one as pre-existing debt, outside this automatic amendment; the new automatic path
preserves accepted versions, including external acceptance during a failing refresh. No new
permission or approval bypass is inferred. No dev-stack restart, deployment, commit or live
provider mutation was performed for this follow-up. Migration 042 is included and tested,
not applied to the running dev stack by this task.

## Committed change review f2b2d9f5 — consolidation before repairs

User requested a commit and code review. Commit `f2b2d9f5` contains the accumulated
PR command/shared-flow/automatic-refresh changes relative to `75b522e7`; the working
tree was clean after commit. Three fresh independent full-scope reviewers inspected
that frozen commit and saved `reviews/pr-commit-{one,two,three}.md` before sharing.
Earlier authorship/account-connection work was governing context. The main agent's
separate source discovery is saved at `/tmp/g5-pr-commit-self-review.md`.

| Finding | Consolidated disposition and required check |
| --- | --- |
| C1 (P2): retained mutation results hide current automatic state | Reviewers one/three and main; accepted. Both source dialogs must follow current same-PR query data after create/update, while preserving immediate mutation results until query catch-up. Mounted success→failure and pending→recovery checks must distinguish stale snapshots. |
| C2 (P2): title acceptance can mutate a concurrently closed PR | Reviewer one; accepted after checking both reuse branches and terminal provider/source early returns. Guard title acceptance against terminal state and check the affected row count; a real close held across the public update must preserve closed title/pending intent and refuse success. |
| C3 (P1): queued explicit writes retain revoked target access | Reviewer two; accepted, reproduced through the actual VM route with persisted unauthorized title after invitation revocation. Protect source/target identity and revalidate/lock current target access after effect-lock waits for both explicit adapters and provider admission. Refusal must preserve local metadata/evidence and prevent provider effects. |
| C4 (P2): a waiting entry can starve the serial automatic worker | Reviewer two; accepted from control flow. Bound per-entry execution, including error recording; preserve durable retry when cancelled. Hold an earlier PR lock while later work advances, then release and verify recovery. |

All initial reports and their dissent/uncertainty are retained. C1 has failing mounted
consumer evidence; C3 has failing real HTTP/DB evidence. C2/C4 initially have source
evidence only. Unisolated pre-existing manual pool saturation/provider-retarget concerns
remain outside these accepted findings. No policy amendment is needed: fixes enforce the
approved authority, terminal-state, visible-recovery and progress contracts. Repairs begin
only after this consolidation; detailed verification and remaining limits follow below.

### Committed review — repair discoveries

The first ordered-lock repair acquired the VM source UPDATE lock before runtime lookup,
whose separate system query takes a SHARE lock on the same workspace. The new VM regression
exposed that self-wait. Runtime lookup now happens before ordered tenant locks, followed by
the existing locked VM/owner/epoch/lease revalidation; no authority check was removed.

Reviewer two's incremental review found a residual C1 identity case: old query PR A can
remain while creation returns new PR B for the same pair. Live query precedence now applies
only to the mutation's PR ID; a shared selector preserves B until its own query record
arrives. Source dialogs have mounted A→B checks, alongside same-ID failure/recovery checks.

## Committed change review f2b2d9f5 — final assessment

Three independent full-scope reviews were consolidated before repairs. Reviewer two
performed incremental repair checks; its raw report retains the later C1 identity case.
Review/repair time and token costs were not measured separately. Original findings remain
above and in the three reports; none was closed by weakening governing intent.

| Finding / commitment | Final path and discriminating evidence | Assessment |
| --- | --- | --- |
| C1: current-state authority and visible push recovery | `useCreateWorkspacePullRequest` cancels prior source/target list reads and seeds matching IDs before invalidating; `currentWorkspacePullRequest` selects same-ID live state with immediate/new-ID mutation fallback in both source dialogs. Mounted success→failure, pending→recovery, same-timestamp error and predecessor A→new PR B cases pass. A real hook test confirms an aborted older read cannot overwrite the accepted result or alter unrelated/list-filtered records. | Repaired; merged-result precedence retained. Timestamp ordering is deliberately not inferred. |
| C2: terminal title acceptance | Shared `store_title_override` checks open status and affected row count in both reuse branches. A real close owner transaction holds the PR while the public title update waits; after close commits the update refuses and the entire closed row remains equal to the close result. Existing overlapping provider-title requests still pass. | Repaired; terminal title/pending intent preserved. |
| C3: current target authority | Shared caller scopes acquire source/target locks in workspace order, using the required VM source strength without later upgrades. Runtime handle lookup precedes tenant locking and is revalidated under the runtime fence. The shared access helper locks invitations and rechecks after local application and provider PR waits, before publication/commit. Actual UI and VM POSTs wait behind a PR lock, lose their invitation, refuse with unchanged metadata/evidence and no provider rows; restoring access permits retry. A separate public provider-phase wait loses access after valid local commit and reports not-attempted admission failure without provider completion. | Repaired; prior authorized source/local effects remain durable as designed. |
| C4: recovery progress | Automatic scopes use a five-second SQL lock timeout, including error recording; maintenance also bounds the complete item to 120 seconds. A real earlier PR lock remains held while a later organization reaches its new published head, then the earlier PR converges after unlock with pending cleared. Existing delayed-provider/later-org and cancellation regressions pass. | Repaired; the outer 120-second safety cap is source-inspected, not separately waited to expiration. Durable inventory/head mismatch retains cancelled work. |

Executed final checks: 46 PR API, five native push and four read API tests pass
(`/tmp/g5-pr-commit-api2.log`, two test threads). All 60 focused UI/hook tests pass
(`/tmp/g5-pr-commit-ui3.log`); build TypeScript check passes. Scoped ESLint has zero errors
and four existing unused-declaration warnings. Rust format and diff whitespace checks pass.
Earlier CLI/migration/worker-surface evidence remains valid because those files did not
change in this repair. No broad rerun of those suites is claimed.

The initial unrestricted backend test run exhausted local PostgreSQL connection capacity
and failed during fixture/pool creation; rerunning with two test threads passed all selected
cases. The initial VM repair test exposed the self-lock described above and passed after
repair. The UI cancellation test initially inherited an infinite-freshness fixture and
therefore started no read; explicitly stale query options made the check discriminating.
React test output still includes act warnings from asynchronous fixture updates.

Implementation and scoped review are complete with no accepted finding left unresolved.
Live GitHub, deployed guests and actual process-restart recovery remain unexecuted; local
Git, restricted-role PostgreSQL, native scripted peers, mocked providers and mounted React
consumers supply the runtime evidence. No push, deployment, running-stack migration or
restart was performed. This assessment records the initial commit and the coherent review
repair; it does not certify unrelated pre-existing governance/P6 gaps.

## Workspace CLI initial design correction

The initial design below proposed a credential-free local PR adapter. Before completing
implementation, the user asked why the terminal could not call g5d directly. The accepted
MVP guest token contract already supports that. The adapter was removed in favor of
shared direct CLI context; no adapter was shipped or tested. Original handoff:

### Workspace CLI usability amendment

Compared with `4dc163da`, add `g5.runtime.workspace-cli` in
[runtime](../../ontology/software_architecture/runtime.g5.md#make-the-workspace-cli-available-to-people-and-agents).
The user authorized implementation of the proposed UX fixes in the current conversation.

```diff
diff --git a/.g5/ontology/software_architecture/runtime.g5.md b/.g5/ontology/software_architecture/runtime.g5.md
index 7c45c067..7e720731 100644
--- a/.g5/ontology/software_architecture/runtime.g5.md
+++ b/.g5/ontology/software_architecture/runtime.g5.md
@@ -110,6 +110,21 @@ Use image pins only from committed restored code-member HEADs, falling back to t

 Supply a complete read-only runtime, provider CLIs, libraries and pinned module catalog independently of the customer image. Reject missing/incompatible components before readiness; no guest-copy fallback. [Module authority](workspace.g5.md#extend-through-declared-modules) governs extensions.

+## Make the workspace CLI available to people and agents
+
+[D · g5.runtime.workspace-cli]
+
+Workspace terminals and provided-agent shells can invoke `g5d` by name through one
+runtime-owned launcher setup, preserving image tools and applying G5 library paths only
+to G5 processes. Terminal PR commands infer the current workspace and use a bounded
+local runtime adapter to the existing [PR operations](../features/governance.g5.md#create-or-update-a-pr-from-a-workspace-agent),
+without another login or copying dispatcher credentials into shell environments.
+The adapter accepts only supported PR operations for its own workspace; it cannot
+forward arbitrary URLs, headers or source identities. Server-side
+[current authority](../concepts/security.g5.md#admit-actions-against-current-authority)
+and [publication credentials](../concepts/security.g5.md#keep-provider-publication-credentials-server-side)
+remain authoritative. Lost replies do not trigger automatic write retries.
+
 ## Host Git never trusts retained guest control files

 [D · g5.runtime.materialization]
@@ -258,6 +273,12 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
 - From [C085/C094](../../uplift/g5-lean/candidates/contracts.md), [S021](../../uplift/g5-lean/candidates/supporting-docs.md); [R03](../../uplift/g5-lean/investigations/runtime-security.md#r03--runtime-bundle-readiness-has-a-narrower-check-than-its-documented-contract).
 - ~65% · runtime bundle/catalog validation; readiness does not establish complete executable, provider CLI, library and pin closure.

+### g5.runtime.workspace-cli
+
+- human-verified
+- [Workspace CLI usability request](../../chat_intent/codex-pr-user-authorship/request.md); local adapter and shared launcher are implementation design decisions preserving the requested credential boundary.
+- Unassessed · terminal launch and PR transport require implementation and boundary verification.
+
 ### g5.runtime.materialization

 - agent
```

**Governing commitments:** `g5.runtime.workspace-cli` owns launcher and terminal PR
transport; `g5.terminal` retains the native owner shell. `g5.security.owner`,
`g5.security.admission` and `g5.runtime.lease` require current owner/incarnation checks
at the API. `g5.security.credentials.publication` keeps GitHub tokens server-side;
`g5.security.credentials.reserved-env` prevents settings from replacing CLI bindings.
`g5.governance.pr.read`, `.write` and `.json` retain existing lookup, publication and
output behavior. `g5.design.minimal-api`, `.reuse`, `.adapters` and
`g5.architecture.commands` require a narrow transport adapter, not another PR flow.
`g5.quality.review.cases` and `.evidence` require real shell/IPC boundaries and honest
limits on provider and live-GitHub evidence.

| Commitment → owner | Allowed case / counterexample | Design conclusion |
|---|---|---|
| Workspace CLI → runtime launcher | Terminal and agent find `g5d` / G5 libraries leak into ordinary image tools | Share a G5-only launcher and preserve other tool resolution. |
| Current authority → existing PR API | Owner requests its current PR / arbitrary source, endpoint or expired incarnation | Runtime binds terminal requests to its workspace; API checks remain unchanged. |
| Shared PR writes → publication owner | CLI invokes UI preparation and publication / terminal assembles its own GitHub request | Adapter calls existing API routes without duplicating publication policy. |
| Recovery → existing PR result | Partial failure or lost response / automatic resubmission of a write | Preserve result/error semantics and require inspection before retry. |

## Workspace CLI usability — consolidated initial review

Three independent full-scope reviews completed against the frozen implementation before
repairs: [one](reviews/cli-ux-one.md), [two](reviews/cli-ux-two.md),
[three](reviews/cli-ux-three.md). Their original reports and uncertainty remain intact.
Review and repair time/token costs were not measured separately.

| Finding | Decision and repair boundary |
| --- | --- |
| U1: the PTY test assumes image Git is `/usr/bin/git` (one, P2) | Accepted. Use a controlled image PATH entry forwarding to the available Git and assert it remains selected. This covers custom image toolchains without requiring distro Git. |
| U2: direct API calls cannot cross Codex's network-disabled sandbox (two P1; three P2) | Accepted as an overbroad usability promise, not a new network regression. `g5.agents.admission.permission-policy` and V9 §5.1 explicitly freeze the selected sandbox and prohibit implicit elevation. Preserve that stronger existing contract; qualify runtime design and agent recovery guidance. Network-disabled agents must report the block and leave the same command to the owner terminal, without falling back to `gh` or changing policy. Adding narrow agent API connectivity would require a separate explicit permission design; it is not delivered here. The original unconditional agent-call outcome remains unimplemented in these modes. |
| U3: ambient umask can make root-owned launcher bin inaccessible to the module UID (two, P2) | Accepted. Set the runtime-owned bin to 0755 on install/reinstall, retaining owner-only write access. Verify a retained 0700 bin is repaired and exercise command execution under a different UID on Linux when feasible. Provider-home ancestors remain private. |

Before repairs: 117 selected tests pass, one existing Linux-VM-only dispatcher test is
ignored. These exercise real terminal/CLI transport against a mock API, launcher isolation,
scripted provider launch and PR CLI behavior. Independent reviewers reran relevant cached
artifacts. Source inspection and a newer local native Codex probe establish the network
limitation; no pinned Linux Codex model/tool execution is claimed. No live GitHub,
deployed guest restart or root-to-module-UID launcher execution had been performed.

### Workspace CLI repair evidence and original direct-call acceptance

The pre-review direct-call acceptance was: “fresh terminal and provider shells find
`g5d`, help/field discovery runs, workspace PR reads and create/update reach the existing
API with inferred identity and no extra login, and errors retain recovery details.”
The network-disabled provider case does not satisfy that original outcome. The design
now explicitly retains the stronger pre-existing permission policy and describes owner
terminal recovery; this clarification is not recorded as implementation of restricted
agent network access.

U1 now uses a temporary image-tool directory forwarding to the available Git and asserts
that this path remains selected after the G5 launcher is added. U3 makes launcher bin
0755 on install/reinstall. The guest-only standard-bin creation sets 0755 only on missing directories it creates;
pre-existing image directories and provider-home parents keep their modes. The same real launcher source was compiled
in an isolated Linux container and executed as UID/GID 1000 from a root-owned retained
0700 bin. Four helper tests pass, including argument/library isolation and that UID
transition (`/tmp/g5-cli-linux-recheck.log`). This is Linux helper execution, not a guest
boot or the dispatcher standard-bin branch.

The repaired real terminal test passes (`/tmp/g5-cli-terminal-recheck2.log`), preserving
mock-authenticated read/create, saved-PR publication error and reserved-context refusal.
The first recheck failed on an expired ten-minute fixture token; the rerun passed with
no token-policy change. Runtime/agent unit recheck passes 71 tests
(`/tmp/g5-cli-unit-recheck2.log`); its initial sandboxed attempt could not bind mock server
ports, then passed with local test permissions. No production permission was widened.

## Workspace CLI usability — final assessment

| Commitment | Actual path and discriminating evidence | Assessment |
| --- | --- | --- |
| `g5.runtime.workspace-cli` | Shared runtime-core installer, agent wrapper and dispatcher PATH; retained-0700 directory, argument quoting, scoped-loader, image-tool selection and Linux UID-1000 execution | Implemented; ordinary PTY verified. Full guest `/usr/local/bin` installation and login-shell resets remain unexecuted. |
| `.workspace-cli.context`, `.credentials.platform-token`, `.reserved-env` | Shared three-key environment builder for terminal and agent.turn; real PTY/CLI matches token/workspace and rejects settings override | Implemented under accepted MVP token custody; provider/dispatcher control secrets absent. No new credential store or proxy. |
| `g5.terminal`, `.security.owner`, `.security.admission`, `.runtime.lease` | Existing terminal admission/identity and unchanged internal API current-authority routes | Inspected; transport checks do not stand in for fresh database transfer/revocation testing. Fixed inherited token contract preserved. |
| `.credentials.publication`, `g5.governance.pr.read`, `.write`, `.json` | Existing CLI transport and shared UI/API operation; real terminal read/create and failed update retain selected JSON and recovery detail | No new PR flow or provider credentials. Mock transport verified; live GitHub unexecuted. |
| `g5.agents.admission.permission-policy` | Unchanged typed policy resolution and native sandbox arguments; primary Codex instructions and shared skill describe restricted-mode block and owner-terminal recovery | Frozen policy preserved. Direct API calls in network-disabled Codex sessions remain unavailable; no narrow API permission mechanism delivered. |
| Agent guidance and delivery | Both primary profiles and shared skill; corrected Claude `/pr`; Claude essential rule appended on every launch and Codex profile copied each turn | Scripted provider launch tests pass. Actual model choice and native provider shell execution unverified. |
| `g5.design.minimal-mechanism`, `.reuse`, `.adapters` | Shared launcher/context, direct existing CLI/API | No additional PR transport service, public route or policy owner. |
| `g5.quality.review.cases`, `.evidence` | Three preserved independent reports; scoped repairs and execution above | 117 selected macOS tests passed, with affected tests rerun after repairs; one existing VM-only dispatcher test remains ignored. Linux helper suite adds the executed root-to-UID boundary. These are not deployed-worker/live-provider proof. |

Both changed shared skill/reference copies remain byte-identical. Global profile parity
still reports the pre-existing unrelated `g5-node-explainer/SKILL.md` mismatch (also present
in HEAD); it was not changed. Formatting and whitespace checks pass. No dev-stack restart,
worker deployment or live GitHub mutation was performed. New guest launcher/context code
requires a rebuilt guest bundle and restart to take effect in running workspaces.

### Incremental review discovery — image directory permissions

Reviewer two's incremental pass accepted U1/U3 and the honest U2 permission limitation,
but found F3 (P2): an unconditional chmod on image-owned `/usr/local/bin` could clear an
existing group-write/setgid mode. The repair now uses atomic directory creation per
standard-path component and applies 0755 only after successful creation; AlreadyExists
preserves the image's mode. G5-owned launcher bin still repairs retained permissions.
This new finding and its original report are preserved. Linux helper/PTY evidence above
is unchanged; full dispatcher standard-path installation remains unexecuted.

The exact directory-creation source fragment, with only its absolute paths relocated to
a temporary fixture, was compiled and run on Linux: an existing 2775 directory retains
that mode and a missing directory under umask 077 becomes 0755
(`/tmp/g5-cli-linux-directory-recheck2.log`). The initial probe preserved the existing
mode but hit Docker overlay EXDEV when renaming the base-image directory; the temporary
fixture removes that test-environment constraint. This is targeted branch evidence, not
full dispatcher/guest installation. Final formatting and diff checks pass.

## Rebase on main 967bf34c — reconciliation

Rebase onto main 967bf34c, preserving branch backup f5f428c9.

Ontology reconciliation before dependent conflict resolution:
- Current tenant/source/target authority from main and PR authorship/current-incarnation/target checks from this branch are cumulative, not alternatives. Preserve both and their race fences.
- Preserve main's Base read-only presentation, organization Home routing, issue-search filters and connected-account lookup while retaining account-level Connect GitHub and PR refresh consumers.
- Preserve main's provider lifecycle, limits, execution-tree containment and root-harness-only deployment (g5.harness.provided, issue 248). Do not resurrect deleted provider profiles, skills, slash commands or MCP surface checks. Carry concise conditional PR operational guidance into the shared root AGENTS.md; CLI help/docs own detailed flags and recovery. Exactly four ontology skills and the two review-round MCP tools remain.
- Preserve automatic PR follow-push, current authority, durable pending title/refresh and exact-commit merge/approval rules. Renumber unpublished branch migrations 041/042 to 046/047 after main's 045; update executable migration references and checksums without rewriting historical evidence.
- Existing metadata changes from main and this branch remain cumulative; relocated implementation references and new evidence are assessed after resolution.

The four original commits are replayed, with obsolete profile changes carried into root
harness instructions and the PR guide. Shared GitHub tests use main's existing RSA fixture.
Initial integration verification is pending; previous runtime evidence is retained only
for unchanged boundaries. The running local dev stack still uses its pre-rebase images;
validation will use isolated test databases rather than migrate that live development DB.

### Rebase discovery — initial evidence before repairs

Two fresh independent reviewers were available (`reviews/rebase-one.md` and
`reviews/rebase-two.md`). Creating the third was rejected by the agent thread limit;
main-agent self-review supplements these passes and is not counted as independent.
Production remained frozen during discovery; runtime batches used isolated databases.

- R1 (P1): both reviewers and self-review found the new Home issue-search consumer of
  the deleted `useStartGithubConnection` export. TypeScript build fails with TS2305;
  23 Home/component tests fail from the missing function. Route recovery through the
  shared account connection, retaining assignee, terms and State in its nested return URL.
- R2 (test integration): the new main `WorkspacePageBasePull` test lacks the branch's
  authoritative `/pull-requests/targets` fixture, so Prepare to Ship never appears.
  Add a representative server target response; do not restore client target selection.
- R3 (documentation): before discovery, corrected the current PR guide's stale assertion
  that deleted worker profiles still deliver a dedicated PR skill. Root harness/help
  now own this guidance; historical reports remain unchanged.

Initial execution: runtime-core/agent-turn unit suites pass 82 tests; migration versions
are unique and all 47 SQL checksums match. Initial UI batch has 175 passing and 24 failing
tests (R1/R2); build stops at R1. API/provider integration batches are still running.
Source inspection confirms main's scope/liveness admission remains at the PR caller
boundary, all eligible owner roles retain required write scope, and VM/continuation
source/target and current-incarnation fences remain in the shared owner. No authority
repair is indicated by this inspection; runtime tests must still establish the affected
public paths.

### Rebase consolidation

Both independent reports are complete and preserved, with the same R1 root cause and
no further concrete production finding. R2 is a missing test API fixture, not a reason to
change the shared target owner. A later self-review discovery (R4) found that the newly
added Claude harness assertion was placed in a fake-provider fixture that deliberately
skips harness installation. Move those assertions into main's existing real harness
launch fixture; retain the fake native-question round-trip test unchanged. This records
the distinct test boundary before repair. No disagreement requires a contract change.
Uncovered boundaries remain live GitHub/OAuth, native model command choice and full Linux
guest installation; the selected runtime batches cover their existing mock/fixture paths.

### Rebase repair and final verification

R1 is repaired by linking Home's connection recovery to `/account/github` with a nested
local return destination containing its selected assignee, query and State. Home no longer
creates a grant; the shared account owner retains OAuth and errors. Existing restoration
behavior is unchanged. The Home test asserts the account destination, preserved filters
and zero Home grant requests. R2 adds the missing baseline server target fixture, retaining
the Base update test's original public interaction/assertions. R4 moves PR guidance/PATH/
context assertions into the real Claude harness fixture, preserving fake-provider semantics.
The pre-fix question test waited for an event the broken fixture could not emit and was
terminated; it and the modified real-harness test both pass after repair.

| Commitment | Actual path and discriminating check | Result |
| --- | --- | --- |
| `g5.settings.github-connection`, main Home routing/search | Shared account route; Home filter round trip, post-login/PR/comment recovery, Base update | 199 UI tests pass across 12 selected files; production TypeScript/Vite build passes. Live OAuth remains unexecuted. |
| `g5.governance.pr.read`, `.write`, `.target`, `.title`, `.json`, `.follow-push`, `.publish.github-author`; `g5.architecture.pr_flow` | Shared API/VM adapters, provider mocks, actual native push, stored PR heads, retry/title/approval state | 46 PR API + 4 reads + 5 native push + 15 GitHub App + 10 review tests pass against main's scope/liveness fences. Live GitHub remains unexecuted. |
| Durable title/refresh after main's schema changes | Ordered 046/047 after main's 045; populated upgrade and replay under non-BYPASSRLS table owner | Upgrade/replay passes; all 47 checksums and unique migration versions pass. Existing live dev DB was not migrated. |
| `g5.runtime.workspace-cli`, `.context`; server credential and reserved-env boundaries | Shared launcher/context; real terminal with mock API; selected command transport | 31 dispatcher + 12 PR CLI tests pass; one existing host/VM-only dispatcher test ignored. Prior Linux helper evidence remains historical, not a rebased guest deployment check. |
| `g5.harness.provided`, provider permissions/lifecycle/limits | Root harness and exactly four skills; native provider-layout fixtures and bounded execution tests | 82 unit tests pass. All 12 selected agent-turn cases pass across initial and affected recheck runs; no parallel injected prompt or legacy profile is restored. Native model choice and full guest boot remain unexecuted. |
| Current source/target authority, one owner per flow, minimal adapters | Existing public/internal typed callers, main scope lock helper, sorted PR locks, original third commit patch-equivalent | Inspected and covered by selected API authority/race tests; no extra transport or permission mechanism added. |

Evidence: `/tmp/g5-rebase-api.log`, `-runtime.log`, `-cli.log`, `-providers.log`,
`-provider-recheck.log`, `-ui-recheck.log`, `-ui-build-recheck.log` and `-fmt-final.log`.
Rust formatting and working-diff whitespace checks pass. Vite reports its existing large
chunk warning. Two independent reports plus self-review were available; a third fresh
context could not be created. Scoped metadata now records Home recovery, root-harness PR
guidance and migration 046/047 references while retaining main's actual delivery gaps.
The running dev stack still uses pre-rebase binaries/images and database history.


## CI and code-review follow-up — initial findings

User request: fix CI failures and code-review comments, with gh access outside the
sandbox. Snapshot 530dc8fb. Retain prior reviews and recheck affected boundaries.

- C1: Rust build 009adbcd and E2E build 1ba5fdbc both fail the route authorization static
  audit at `create_workspace_pull_request path.id`: the adapter delegates to the shared
  typed owner but the audit recognizes only the earlier route-local guard. Existing
  HTTP authority tests establish the actual guard; update the static service proof too.
- C2: 4151637091 is valid: outer timeout drops the error-recording future. Move the
  deadline to the shared refresh owner; record a sanitized pending error and notify only
  after commit, retaining current-owner/terminal/newer-completion protection.
- C3: 4151637094 is valid: replicas independently repeat the open-PR discovery/preflight.
  Use a dedicated session advisory lock for loop ownership, released on shutdown/error;
  retain per-PR locks and periodic recovery. Test contention and cancellation takeover.
- C4: 4151774354 is valid: failed target loading empties targets and hides/disables every
  action that could show the error. Add visible retry recovery independent of dialogs.
- C5: 4151774350: update the embedded governance migration references to 046/047, retaining
  historical execution evidence in this review.
- C6: 4151637078 targets the removed legacy Codex profile and is obsolete after rebase;
  root harness instructions and exactly four ontology skills replace it.

CodeRabbit's generic docstring percentage warning is advisory and does not identify a
specific behavioral defect. Add useful documentation to changed boundaries; do not pad
unrelated functions to satisfy a blanket percentage. Cloud Build frontend, browser and
provider end-to-end checks passed; their overall failure is the shared route audit.

### CI repair review consolidation (before second repair)

Three independent reviewers of this increment saved `reviews/ci-fix-one.md`,
`ci-fix-two.md` and `ci-fix-three.md` before sharing. They did not author these changes;
reused contexts retain earlier, different review increments. Production stayed frozen
through discovery. All findings are source-inspected counterexamples, with initial
runtime evidence supplied separately by the main agent (48 PR API tests, 8 static
checks, target-query retry test and production UI build pass).

- R-C1 (P1): the failure-recording phase is outside the publication deadline. Scope
  admission can block on membership locks before its local lock timeout is installed;
  the newer-completion check can also repeat stalled Git reads. Both causes violate
  later-PR progress under persistent scan leadership. Bound the entire reconciliation
  phase, including admission and completion checks, while preserving fresh authority,
  closed-PR/newer-success guards, and commit-before-notify. A locked-membership worker
  regression must prove another PR progresses before the lock is released and the
  blocked PR recovers afterward. Storage stall injection remains a distinct evidence gap.
- R-C2 (P2): the new destination error banner uses live-edit eligibility, hiding retry
  for stopped owners who may create PRs from durable state. Use PR owner eligibility
  and test running and stopped owners without starting the VM.

The minority stopped-owner finding is retained; agreement is not the basis for its
validity. No other concrete defect was found in the shared typed authorization proof,
leader cancellation, or migration references. Mid-scan connection loss and real provider
cancellation remain unexecuted boundaries. Strict local Clippy additionally found two
branch warnings (empty-string comparison and an eight-argument provider helper); inspect
CI policy before deciding whether either needs repair. ESLint has zero errors.

### Later discovery during runtime verification

The new held-membership regression still times out with the whole Rust recorder bounded.
Inspection exposes a second cancellation boundary: SQLx queues rollback when a transaction
is dropped, but a blocked server query still owns the connection until that query ends.
Both timed-out attempts can occupy the small tenant pool and prevent the later scan's
admission. Install a transaction-local PostgreSQL lock timeout before the shared actor
helper locks scope authority for continuations; retain the same capability and scope
checks. This refines existing bounded-admission policy, without widening authority.

The first unconstrained 49-test run also had 12 fixture failures (500 responses during
initial PR creation); limiting local test threads to two yields 48 passing and only this
new discriminating regression failing. This suggests local resource contention, rather
than treating the fixture failures as evidence against the repaired behavior.

### CI repair final verification

C1–C5 and R-C1/R-C2 are repaired; C6 is obsolete because main removed the legacy profile.
The public route still calls the same typed publication owner. The audit now recognizes
that boundary and checks path/caller calibration plus admission ordering. Failure reporting
has a separate five-second budget, and the shared actor admission helper sets a transaction-
local SQL lock timeout before scope locks for continuations; existing HTTP/VM callers keep
their prior admission behavior. The dedicated scan connection owns the advisory lock across
ticks and releases it on cancellation. Owner-visible destination retry uses the server query
and includes stopped workspaces. Embedded migration references match 046/047.

| Commitment | Authoritative path / discriminating check | Result or gap |
| --- | --- | --- |
| Shared PR flow, current authority, `g5.governance.pr.write` / `.target`, `g5.architecture.pr_flow` | Real public/VM PR routes; target/identity revocation and partial provider recovery; typed static route calibration and actual service guard ordering | 49 PR API tests and all 8 route-auth static tests pass. The exact check failing both Cloud Build jobs passes. |
| `g5.governance.pr.follow-push`, `g5.design.commit-notify` | Timed-out real PR-row wait, stored pending/error fields, actual WebSocket event, subsequent persisted heads | Pass. Terminal/current-owner/newer-completion guards retained by inspection and existing API race coverage; exact new-recorder ownership/close races are not separately injected. |
| `g5.architecture.recovery.progress` | Held membership UPDATE lock through complete worker scan; later org's persisted heads advance before unlock; first PR retries after unlock | Pass after the server-side admission deadline repair. The regression failed before repair. It passed with a stricter 12-second test bound; source uses 20 seconds for CI scheduling headroom with the blocker still held. Stalled-filesystem injection remains unexecuted. |
| Replica discovery cost / recovery | Actual background loop holds ownership across ticks; competing one-shot scan skips; cancelled leader releases lock and takeover publishes new heads | Pass. Mid-scan connection-loss/provider-cancellation injection remains unexecuted. |
| Visible target-query failure/retry, durable stopped variant | Mounted running and stopped owner pages: HTTP failure → visible retry → same query succeeds → Create PR enabled; no VM start | Both pass; Base update regression also passes (3 tests total). Production TypeScript/Vite build passes. |
| Implementation quality | Strict `cargo clippy -p g5-api-core -p g5-api --tests -- -D warnings`, formatting, ESLint, whitespace | Pass; ESLint reports 94 existing warnings and zero errors. Empty-string comparison and needless borrow fixed; the provider helper's existing eight-argument context is explicitly documented/allowed. |

Evidence: `/tmp/g5-pr268-fixes-api5.log`, `-destinations4.log`, `-fixes-build3.log`,
`-clippy4.log`, and `-eslint-final.log`. Three initial reviewer reports are preserved,
with an incremental repair recheck in `reviews/ci-fix-one.md`; reviews are source inspection
and main owns runtime evidence. Scoped ontology metadata records current evidence and
limits without changing governing contracts. Remote CI has not yet rerun on this repair.
Live GitHub/OAuth, deployed replicas/guest runtime and full native-model behavior remain
outside this follow-up's executed coverage. The live dev stack was not restarted or migrated.

### Continuous CI follow-up — next failed head

User asks to loop over CI and new review findings until PR #268 is clean. On 6bff7802,
CodeRabbit succeeds and all five review threads are resolved. Rust runtime lanes all pass,
including 1,021 API tests, but its aggregate fails RUST_CLIPPY. The end-to-end aggregate
stops at the frontend suite: 3,612 tests pass and the raw-button test-ID guard fails
because the destination retry button lacks its required `data-testid`. Preserve the
button behavior and add its stable test anchor. Retrieve exact Clippy diagnostics before
repairing that separate cause. These are bounded compatibility fixes within existing intent.

Exact Rust diagnostic: `clippy::items_after_test_module` in `routes/workspaces.rs`;
`get_pull_request_info` and `read_pull_request_info` follow the module. Move both unchanged
handlers before the test module, preserving authorization and query behavior. The prior
local `-p g5-api` Clippy run did not build the dependency crate’s test target; verify with
CI’s workspace/all-targets command in this follow-up.

Both guard repairs pass: all 10 selected frontend tests (raw-button coverage plus
running/stopped destination retry and Base update), and CI's exact Clippy scope
`cargo clippy --workspace --all-targets -- -D warnings` on the local macOS target.
The moved PR handlers are byte-for-byte unchanged apart from their position. Formatting
and whitespace checks pass. No behavioral contract or ontology assessment changes are
needed for these two mechanical fixes. Remote Linux CI and automated review will be
rechecked on the next pushed head; this is not a remote-green completion claim.


## Rebase onto main 72d2b4de — reconciliation and initial assessment

User asked to check main's migrations and rebase if needed. Starting head b1d3342e is
preserved at `codex/pr-user-authorship-pre-rebase-b1d3342e`. Main had advanced 54 commits
and introduced migrations 046–051, colliding with the branch's 046/047. Before resolving
code, compared affected governance, durability, tracking, security and migration intent:
selected-file Save/review-acceptance pushes and automatic divergence completion compose
with existing PR follow-push while retaining push policy and current authority. No
contradictory intent was found. Main's acceptance commitments and branch's PR commitments
auto-merged; the authority footer conflict preserves relocated links and PR evidence.

Code conflict was tracking error presentation: retain main's design token and the branch's
readable merge_active explanation. Rename only branch migrations to 052/053, keep SQL and
hashes unchanged, reconcile checksums and advance the populated upgrade test's baseline.
The remaining commits apply without textual conflicts. Main's historical 047/048 owner
execution remains unchanged. Update governing metadata and current embedded migration
references; prior review receipts remain historical. Runtime and incremental review are
pending; the live dev stack has not been migrated or restarted.


### Rebase consolidation

Three independent incremental reports (`reviews/rebase-72d2b4de-{one,two,three}.md`)
find no actionable rebase defects. Their reused reviewer contexts and inspection limits
are disclosed; none authored this reconciliation. All independently checked migration
integrity and material shared-flow interactions. No code repair or contract amendment
is indicated. Main-owned execution passes 99 API/migration/static checks, all 3,801 UI
tests, production build, full workspace/all-targets Clippy and formatting. Native CLI
checks initially failed because the local g5d binary could not load `libkrun.1.dylib`;
direct execution reproduced the loader error and succeeds with Homebrew's library path.
That failed run was stopped; repeat with `DYLD_LIBRARY_PATH=/opt/homebrew/lib` is pending.
The initial failure is an environment limitation, not rebase correctness evidence.

### Rebase final verification

The configured native rerun passes all 44 dispatcher and 12 PR CLI tests with no failures
or skips. This includes real child process/harness and native push/selected-file Git
boundaries under fixtures; it is not live-model or deployed-guest evidence. Final checks:

- 99 API/migration/static tests: 49 workspace PR, 4 PR read, 15 GitHub bindings,
  11 workspace review, 11 migration owner cleanup, 1 populated 053 upgrade/replay,
  and 8 route authority guards.
- Entire frontend suite: 242 files / 3,801 tests; production TypeScript/Vite build passes.
- Full `cargo clippy --workspace --all-targets -- -D warnings`, Rust formatting and
  whitespace checks pass. Vite retains its large-chunk advisory.
- All 53 migration versions are unique and all SHA384 entries match. Main's 001–051 SQL
  and branch's renamed 052/053 SQL are byte-identical to their original sources.
- Range-diff preserves four of seven commits exactly; other differences are the two
  resolved presentation/reference contexts and migration/test renumbering.

Evidence: `/tmp/g5-pr268-rebase72-{api,ui,build,clippy,cli2,fmt}.log` and the saved three
review reports. No actionable review finding remains. Live GitHub/OAuth, full guest
installation, and a combined live selected-editor-Save → GitHub PR observation remain
unexecuted. The live dev stack was neither restarted nor migrated. Main and remote branch
were rechecked before publishing; main remains 72d2b4de and the lease expects b1d3342e.
The existing CI/review follow-up continues against the new head; remote checks are distinct
from these successful local checks.

### Automated review follow-up — runtime helper placement

At b4abcb10, Rust CI succeeds and end-to-end CI remains in progress. Review comment
4157638678 is valid: the new `currentWorkspacePullRequest` runtime function conflicts
with `lib/types.ts`'s explicit type-only module rule. Move that helper unchanged to
`lib/workspacePullRequests.ts` and update its two dialog consumers. Other existing
runtime helpers predate this change and are outside this bounded repair. No PR policy,
selection behavior, authorization or ontology meaning changes. Existing mounted-dialog
freshness/replacement tests and a production build will verify the relocation.

Relocation verified: both consumers import the runtime module; the function body is
unchanged and its summary dependency is a type-only import. All 45 mounted PR-dialog
checks, production TypeScript/Vite build, scoped ESLint, and whitespace checks pass.
Evidence: `/tmp/g5-pr268-helper-{tests,build,eslint}.log`. Self-review finds no changed
selection or failure behavior and no additional runtime re-export from `types.ts`.
Remote E2E is still running on the preceding head, so the monitoring loop remains active.


## Rebase onto main 91bc4742 — reconciliation and initial assessment

Current remote head 1ce454d7 passed Rust, E2E and CodeRabbit with zero unresolved
threads, but the final mergeability check found new main conflicts. Preserve that tip at
`codex/pr-user-authorship-pre-rebase-1ce454d7`. Main adds comment migrations 052–054,
thread authority/attribution, principal-fenced comment requests, retained transient-refresh
page state, and after-commit notifications. Read/reconcile these commitments before code:
review suggestion hand-back stays separate from PR publication, comment principal binding
retains main's explicit scope, and both WebSocket/update consumers survive.

Preserve all main SQL; rename branch migrations/test to 055/056, without changing SQL,
and advance the populated baseline. Union independent ontology additions with their
metadata; keep main's corrected review-handback and notification assessments. Place the
PR destination retry inside main's composer wrapper while retaining the separate transient
refresh status and stable retry test ID. Remaining code applies automatically; verify the
combined consumers rather than assuming textual compatibility. One overly broad initial
conflict-edit attempt was discarded by aborting/restarting before retaining the rebase.

Current plan and scoped migration evidence are updated. Runtime checks and three
incremental reviews are pending. The existing dev stack was not restarted or migrated.

### Rebase 91bc4742 consolidation — before repair

Three fresh, independent full-scope incremental reviews saved raw reports at
`reviews/rebase-91bc4742-{one,two,three}.md` before sharing. Production remained frozen.
Review two found no defect and independently ran 129 frontend checks. Consolidation
retains three reproduced P2 findings from reviews one and three; absence in another
report does not refute their concrete counterexamples:

- R91-1: a PR update while the first PR list request is outstanding reuses the old
  request, so its stale answer clears invalidation and no newer evidence is read.
- R91-2: workspace reconnect omits PR queries, leaving missed automatic refresh/error
  events stale indefinitely. Preserve subscription scope and use the existing
  `invalidateAndReadAgain` helper for both event and reconnect paths. Inspect its actual
  cancellation/refetch behavior; it cancels first reads and invalidates active queries,
  without a second publication path or broader authorization.
- R91-3: a transient PR destination refresh drops loaded choices, resets explicit Create
  PR selection and unmounts Prepare to Ship, losing its description. Reuse existing
  `toRefreshingState` (loaded transient failures retained; initial failures and refusals
  still fail), expose stale status/retry, and verify actual page draft/selection behavior.

All reviewers confirm immutable migration SQL/checksums and compatible owner/authority
reconciliation. Main runtime checks: 135 API/migration/static tests, production UI build,
workspace/all-targets Clippy, formatting, scoped ESLint (existing warnings only), and
migration/ontology integrity pass. The complete frontend run passed 4,036 of 4,040 tests
with four timeouts during concurrent compilation; an unchanged serial rerun of the four
files passes all 284 tests. Contention is plausible, not proven; retain the initial receipt.
Reviewer probes independently reproduce the three consumer failures despite those green
ordinary checks. Add discriminating page and active-query/socket regressions before repair.
Server, native and migration checks remain valid for these frontend-only repairs.

### Rebase 91bc4742 final verification

R91-1/R91-2 reuse the existing cancellation/invalidation helper for PR events and
workspace reconnect. R91-3 uses retained destination query state and one shared
status/retry element rendered inside the active PR dialog or in the page when closed.
No new query, publication operation, persistent store or authority is introduced.

| Obligation | Actual path and discriminating verification | Result |
| --- | --- | --- |
| Current PR evidence after an event overtakes first read | Real WebSocket manager plus active query observer, source/target/all lists, late old answer and unrelated workspace | All three regression cases fail before repair and pass afterward; original independent review-one probe also passes unchanged. |
| Recover updates missed while disconnected | Actual socket close/reconnect/open with loaded PR lists and outstanding first reads | Both regressions fail before repair and pass afterward; prior thread recovery remains covered. |
| Keep loaded destinations, draft/selection and visible retry | Mounted WorkspacePage with actual hooks: 503 during Prepare to Ship description / explicit Create target; retry inside dialog; actual target removal | Both cases fail before repair and pass afterward. Hook regression retains 503 data and rejects 403/404 data; running/stopped initial-failure controls pass. Original independent review-three retention probe also passes unchanged. |
| Server current authority, migration evolution and shared publication | Existing authorized public/VM services, real isolated DB/Git/provider fixtures, populated migration upgrade/replay | 135 API/migration/static tests pass. Includes 31 comments, 16 review, 49 workspace PR, 15 GitHub binding, 4 PR read, 11 historical migration-owner, 1 populated 056 and 8 route guards. Frontend repairs leave these paths unchanged. |
| Combined frontend | Full suite after repairs, two workers and no simultaneous compilation | 253 files / 4,049 tests pass. Earlier four timed-out files also passed unchanged in isolation. Scoped ESLint has zero errors (100 existing warnings across the expanded scope). |
| Minimality, ontology and migration integrity | Three independent raw reviews, main consolidation, two repair rechecks, body/footer IDs, 56 unique SHA384 versions and byte comparisons | No new actionable finding; main's 001–054 and branch's renamed 055/056 SQL remain unchanged. Scoped evidence footers and saved plan updated together. |

Full workspace/all-targets Clippy with warnings denied, Rust formatting and whitespace
checks pass. The final production TypeScript/Vite build passes; Vite retains its
large-chunk advisory. Logs: `/tmp/g5-pr268-rebase91-` prefix
with `api.log`, `ui-final.log`, `regressions-before.log`, `retention-before.log`,
`regressions-after.log`, `clippy.log`, `eslint-final.log`, and `build-final.log`.
Reviewers' raw reports/rechecks remain alongside this record. Existing native CLI evidence
is retained because these main/reconciliation changes do not alter native PR dispatch;
remote CI will execute applicable native checks again. Live GitHub/OAuth, full guest
installation, filesystem stalls and mid-scan connection loss remain unexecuted. The live
dev stack was not restarted or migrated. Remote checks must rerun on the new head.

## Manual retry without migrations — initial verification

Scope: current amendment in `ontology-plan.md`, implementing the user's acceptance of
manual retry and removal of migrations 055/056 on top of 4504311b. Initial discovery
snapshot is the uncommitted delta; three fresh full-scope reviewers are working read-only
and will save reports before consolidation. No peer or author findings were supplied.

Self-review/check evidence before repair:
- Migration directory and checksum manifest exactly match base 91bc4742; branch SQL
  additions and the obsolete 056 upgrade test are removed. Fresh isolated test databases
  build successfully using 001–054.
- GitHub binding tests: 15/15; PR read tests: 4/4; workspace PR tests: 46/49. Three failures
  share one issue: the `blockingMessage` projection includes informational successful
  preflight text. New consumers interpret any message as failure, so healthy PRs would
  show an error. Required repair: only actual failures or blocked preflight can populate
  this projection; successful retry must clear the warning. Keep the discriminating
  public-read assertions rather than accepting informational text as error state.
- Compiled CLI PR tests 12/12; focused UI/cache/page tests 80/80 across seven files;
  TypeScript/Vite build passes; scoped ESLint has zero errors and one pre-existing
  component-export warning. These mocks do not cover the real preflight projection bug.
- The title retry test now also checks a newly advanced, already-included provider member
  through the actual title-less Update PR route; that extension was added after the first
  API compilation and must run on the next pass.

Logs: `/tmp/g5-pr268-manual-retry-{api,cli,ui,build,eslint}.log`.

## Manual retry — consolidated discovery before repair

Fresh reports: [one](reviews/manual-retry-one.md), [two](reviews/manual-retry-two.md),
[three](reviews/manual-retry-three.md). All reviewed the same full delta without author
history or peer findings; reports are retained unchanged. Source evidence confirms four
causes; agreement counts are not used as evidence:

1. **P1 completed-member evidence:** newly reconciled included members feed completed-PR
   refusals through the candidate upsert, replacing pinned and observed provider commits.
   Retain successful pins/proof on failed attempts and refuse recorded completed members
   before the provider owner's publication effects. Verify real public and VM retries.
2. **P2 false error after success:** informational ready-preflight messages populate
   `blockingMessage`; confirmed by three executed API failures. Only blocked preflight
   or a real incomplete-publication outcome may populate it.
3. **P2 failure hidden by local refresh:** arbitrary latest-event ordering allows the
   local-only `/refresh` route to hide automatic errors. Distinguish complete shared-flow
   outcomes from local evidence events; order those outcomes at actual write time under
   the existing PR lock. Manual and automatic success must settle the same error surface.
4. **P2 missing provider member:** current local evidence plus no provider row can make
   automatic discovery falsely conclude completion. Reuse actual candidate eligibility
   to detect a required missing provider PR, preserving no-empty-PR behavior.

The plan contains one inaccurate ID (`g5.governance.pr.readiness`); correct it to
`g5.governance.publish.readiness`. No new authority or pending state is justified.
Snapshot before repair: `/tmp/g5-pr268-manual-retry-before-review.patch`.

## Manual retry final assessment

All consolidated defects are repaired. The independent [recheck](reviews/manual-retry-two-recheck.md)
finds no remaining actionable defect by source inspection; execution evidence is separate below.
The new terminal regression initially returned 500 because this automatic-only fixture had no
`origin` for explicit source publication. Configuring its real source remote fixed the fixture;
both UI and VM requests now pass without weakening the authority or evidence assertions.

| Obligation | Actual owner and discriminating check | Final result |
| --- | --- | --- |
| Remove new schema/pending consumers | Migrations/checksums equal base 91bc4742; fresh TestApp databases; removed-field search across backend/frontend/current docs | No PR migration/schema delta; 001–054 unchanged; no live pending-field reference |
| Shared UI/CLI flow and current admission | `workspace_pull_request_create`; real user/VM HTTP, save/push, owner/target/incarnation and queued-race cases | Existing single flow retained; API and compiled CLI checks pass |
| Successful pushes / approval version / exact merge checks | Committed continuation and existing provider/merge owners; native receive-pack, multi-member refs, unchanged approval and source-race checks | Existing regression coverage passes; no implicit save/start/approval |
| Useful errors and manual recovery | Existing provider errors plus vector-bearing full-flow outcome events, `blockingMessage`, UI consumers | Deadline failure is visible after commit; real local-only Refresh cannot clear provider failure; Update PR clears it after successful synchronization |
| Request-scoped title | Stored G5 title plus explicit request under PR lock; failure/resubmission/omission, external edits, stale and concurrent requests | All pass; failed titles require resubmission and are not replayed implicitly |
| Included/terminal provider members | Existing provider candidate owner and status writers; new public/VM retry cases assert all four persisted SHA fields and merged status | Open included members receive current pins; completed members retain accepted pins/proof; no empty/new provider PR or provider PATCH/POST on terminal retry |
| Missing provider outcomes / bounded progress | Shared candidate eligibility and maintenance; included no-row no-op followed by pushed outgoing member and local Refresh; held-lock, later-org, deadline and leader tests | Missing member discovered without pending state; existing progress cases pass |
| Notifications / cache state | Commit-before-event shared owner, actual timeout WS event/public read; active queries/socket/page tests | Committed errors and repairs reach supported consumers; prior cache/destination recovery retained |
| JSON and UI compatibility | Removed fields deleted from Rust/TypeScript/CLI field discovery; supported `blockingMessage` and provider outcomes retained | CLI, UI/cache checks and production build pass |

Final executed checks:
- **77 API/static tests:** 50 workspace PR, 15 GitHub binding, 4 PR read, 8 route-auth.
- **74 CLI unit tests** selected by the PR filter and **12 compiled PR CLI tests**.
- **80 frontend tests** across the three changed consumers plus cache/socket/hook/page suites.
- Production TypeScript/Vite build; full workspace/all-target Clippy with warnings denied;
  Rust formatting; scoped ESLint (0 errors, 1 existing export warning); whitespace checks.
- All 54 retained migration SHA384 checksums pass; no migration diff against base 91bc4742.

Final logs: `/tmp/g5-pr268-manual-retry-api-verified.log`, `-clippy.log`, `-json.log`,
`-cli.log`, `-ui.log`, `-build.log`, `-eslint.log` (same `/tmp/g5-pr268-manual-retry` prefix).
No claim is made that an arbitrary source advance after provider acceptance permits immediate
merge import: the terminal regression proves preservation of recorded evidence, while current
merge admission still applies. Live GitHub/OAuth, deployed guest execution and process restart
remain unexecuted. The running dev stack was not migrated or restarted. New remote CI starts
only after this follow-up is pushed; the prior clean head does not verify this new head.

## PR checks — initial implementation verification

Incremental scope is the `pr checks` amendment and implementation above `fa1685cc`.
The user authorized the saved design. Existing publication behavior is outside the
mutation scope; the addition reads through shared public/incarnation admission and
narrow provider credentials. Three fresh full-scope reviewers will inspect the same
working snapshot without author/peer findings, then save independent reports before
main-agent consolidation and one repair pass.

Self-check evidence before independent discovery: API read suite 8/8; compiled PR CLI
16/16. The newly added stored multi-repository case is still pending execution. Local
mock server and database tests require execution outside the network sandbox; initial
sandbox-denied runs are environmental failures, not product evidence. A config fixture
initially used a nonexistent composite conflict target; its existing workspace key is
now used, and the secret-redaction case passes. A filtered CLI unit run passed 72 cases
and hit two unrelated host mock-port sandbox failures; rerun outside the sandbox is
pending. No live GitHub, deployed guest or migration change is claimed.

## PR checks — consolidated findings before repair

Three independent full-scope reports were saved before sharing: [one](reviews/pr-checks-one.md),
[two](reviews/pr-checks-two.md), [three](reviews/pr-checks-three.md). Source remained frozen
until all reports were saved. Main consolidated the evidence, not reviewer votes:

1. **Stale reruns:** all reviewers reproduced queued suites retaining old successful runs.
   Inspect bounded suites and run identities, and revalidate observations before terminal output.
2. **Test-merge omission:** reviewer one found head-only reads could omit failed merge CI;
   reviewer three identified the same open design assumption. Resolve it by reading the live
   open PR's test-merge revision as well as its head, with separate actual commit identity.
   Unknown mergeability remains pending; never reuse a stale merge SHA. This refines the
   saved revision contract before dependent code, rather than weakening it.
3. **Lost diagnostics:** all reviewers reproduced annotation errors erasing available text.
   Preserve summary/text and separately report incomplete annotations.
4. **Rate-limit continuation:** reviewers one/two executed early repolling after annotation
   429; preserve a structured failure, stop provider fanout and CLI polling.
5. **Redaction corrupts protocol:** reviewer three reproduced secrets named pass/ready
   replacing trusted enums. Redact provider-controlled text, preserving derived protocol.
6. **Main discoveries:** Ctrl-C at the request/wait boundary can be lost; keep one pinned
   cancellation future. Clippy rejects unnecessary unwrap; use matching. Also verify stored
   provider repo UUID against attached identity, not only its cached repository name.

Additional boundaries: bounded suite enumeration prevents the check-runs endpoint's hidden
1000-suite limit from claiming completeness; preserve distinct App identities and validate
same-head changes. No new persistence or provider write authority is justified.
Pre-repair receipts: /tmp/g5-pr-checks-before-repair.patch and
/tmp/g5-pr-checks-before-repair.rs. Executed pre-repair checks: 82 API/static, 16 compiled
CLI, 74 CLI unit, 6 harness tests passed; workspace Clippy failed on the unwrap.
Live GitHub/VM execution remains unverified.

## PR checks final assessment

Implementation and independent repair recheck are complete; no actionable review finding remains.
The initial three full-scope reports and [original handoff](reviews/pr-checks-design-handoff.md)
are preserved. Main owns consolidation and repair; recheck evidence is separate from
executed tests. No live GitHub or deployed-VM execution is claimed.

| Obligation | Authoritative path and discriminating evidence | Assessment |
|---|---|---|
| Workspace/provider selection, JSON and no hidden member success | Shared attached repository selector/read owner; API default/UUID/GitHub selection, cached repo UUID mismatch, multi-member failure; compiled CLI projection/exit tests | Passed |
| Read authority and credentials | Existing user/incarnation route admission, repository resolver, effective secrets and exact read permission tuple; outsider/stale VM/unauthorized repo and missing permission API tests; static route checks | Passed; deployed grants unverified |
| Expected source and actual check revision | PR head/base/test-merge vector and row commitOid/revision; API expected-head mismatch, changed head/base, unknown mergeability and head-pass/merge-fail cases | Passed; observations do not lock provider state |
| Rerun and provider identity | Bounded suites/runs/statuses; current suite states and before/after snapshot comparisons; queued rerun, same-head transition and same-name distinct App fixtures | Passed; live provider reruns unverified |
| Details, bounds, redaction and provider failures | GitHub-only GETs, summary/text and scoped annotations; 1000-entry suite cap, truncation, redirects, token/configured secret tests (including pass/ready), 503/429/403 partial diagnostics and fanout stop | Passed; direct build logs outside scope |
| Watch outcomes and cancellation | Entire-result exit decision before JSON; compiled CLI cases for pass/skip/fail/cancel/pending/empty/unavailable, watch progress/final JSON, fail-fast, rate limit and SIGINT during read/wait | Passed |
| Publication, approval, lifecycle and ownership | Shared read path performs no publication/CI mutation or compute start; prior 50 PR publication tests plus scoped credential/static suites | Passed; no migrations or new durable state |
| Worker guidance and minimal architecture | Existing root AGENTS.md is the packaged worker instruction owner; 6 harness tests passed before repair, packaging code unchanged; single shared reader and thin CLI | Inspected and tested; guest rollout unverified |

Executed after repair: **86 API/static** (the 13 read cases include the final active-suite repair), **18 compiled CLI**, **74 CLI unit** tests.
Logs: /tmp/g5-pr-checks-final-api.log, -final-cli.log, -final-cli-unit.log.
The initial rerun fixture accidentally created its App identity twice; the duplicate
setup was removed. The cancellation test initially used an unsupported assert_cmd
method; its standard process helper now passes. These fixture failures are superseded
by the executed results above.

Unrelated concurrent workspace-agent parity files and ontology changes are excluded
from this task's staging and evidence claims. The running dev stack is unchanged.

### Repair recheck discovery

[Independent recheck](reviews/pr-checks-two-recheck.md) found one P2 regression before
further edits: incomplete suites incorrectly masked current failed jobs as pending,
suppressing diagnostics and fail-fast. Original queued-rerun, partial-output, rate-limit,
redaction and test-merge defects passed isolated adapter rechecks. Repair must distinguish
queued rerequest evidence from an ordinary in-progress suite: preserve current failures
while retaining a separate pending suite observation. Add a public/VM failure-plus-running
case and retain the paired queued-rerun/current-transition tests. This does not weaken
the original failure-visibility or revision contract.

Final focused disposition: the [independent recheck](reviews/pr-checks-two-recheck.md)
closes R2-R1 after executing seven provider scenarios and feeding the exact active-suite
observation to the compiled CLI. Nonwatch and fail-fast each made one read, exited 1
and preserved failure details; queued-rerun protection remains pending. All 13 real API
read tests and all 18 compiled CLI tests pass after the final repair. The unchanged
shared selector helper was moved before its test module for Clippy's item-order rule.
Final focused logs: /tmp/g5-pr-checks-active-suite-api.log and
/tmp/g5-pr-checks-complete-cli.log; 15 binding, 50 publication, 8 static, 74 CLI unit and
6 harness results remain valid for unchanged consumers.

Final gates pass: full workspace/all-target Clippy with warnings denied
(/tmp/g5-pr-checks-complete-clippy.log), Rust formatting and whitespace checks.
The supported `g5d pr checks --help` surface was also exercised directly. No schema
migration or dev-stack restart was performed. Remote checks will verify the pushed head.

## CI follow-up after 384d79ab

GitHub reports Rust CI success; E2E build `8211f73d-6f19-4abb-8296-31248e1e1481`
failed only in the frontend test lane. The Cloud Build log identifies
`WorkspacePageChatAccount.test.tsx` / "keeps an Account picked while a later turn is
sending": its first picker action could not find `chat-account-select`. Its readiness
wait selected `claude-model-control`, which also exists before the requested existing
chat loads. The test now waits for `chat-panel-send`, the existing-chat composer, as
the file's held-detail helper already does. Account retention assertions are unchanged;
no product behavior or timeout is changed. This is a bounded fixture synchronization
repair within the existing account contract.

Self-review followed the new/existing composer paths in `ChatPanel.tsx` and the real
page consumer. The failed test and two locally timed-out suites pass together: 90
tests, including V8 coverage as in CI; scoped ESLint, Prettier and whitespace checks
pass. A full local Node 26 run had two runner/time-budget failures (React transform
fetch and a synchronous projection test); those were not the CI Node 22 failure and
passed on the focused rerun. Remote CI is still needed to confirm the new head.
Receipts: /tmp/g5-pr268-384d-e2e.log and /tmp/g5-pr268-ci-focused-fixed.log.
The user explicitly authorized Cloud Build log reads for this PR's CI diagnosis
after automatic approval review initially confused that access with the separate
product feature for direct VM-agent Cloud Build access.

## Rebase 93d7c39e and migration history

Incremental self-review of the rebase from main `91bc4742` to `93d7c39e` found no
implementation or contract defect. The only textual conflict was the migration
checksum list: retain main's `055_review_suggestion_edits.sql`, then apply the
previously authorized removal of the PR's title/refresh migrations. Range-diff
showed all other commit patches unchanged. Every main ontology addition/removal
matches the corresponding rebased delta, including paired metadata; no governing
text or evidence assessment required amendment.

| Commitment / interaction | Actual path and check | Result |
|---|---|---|
| Manual retry without new schema state | Compare each surviving PR commit's migration directory with main; migration runner CI guard suite | Identical directories; 7 guard tests passed |
| PR reads, checks, publication and current authority | `api_pull_request_reads`, `api_workspace_pull_requests`, `route_auth_static`, including main's review lock-order audits | 13 + 50 + 10 passed |
| CLI JSON, watch, partial failure and workspace context | Compiled `pr_commands` plus real dispatcher terminal read/create/refused-update flow | 18 + 1 passed |
| Shared CLI launcher versus main's provider lifecycle | Native default-limit/input-acceptance dispatcher regression with both providers; inspect shared launcher/environment and unchanged main background/End owner | Passed; live provider/guest behavior remains unverified |
| Main's chat controls and PR UI consumers | Full TypeScript check; WorkspacePageChatAccount, WorkspacePageBasePull, CreatePullRequestDialog, PullRequestList and useWorkspace suites | TypeScript and all 54 tests passed |
| Ontology reconciliation and history preservation | Compare main/rebase ontology deltas and final tree before/after squash | All six ontology deltas preserved exactly; squash leaves final tree unchanged |

The user requested eliminating transient migration additions from PR history.
The shared PR implementation through its explicit title-retry documentation fix
was folded into one commit; authorship, check reads and the frontend CI fix remain
separate. Both pre-rebase and pre-squash local backup branches are retained. The
reconciliation record is included with the final frontend verification commit.
No new product behavior, schema, deployment or live dev-stack restart is introduced.

The first native terminal/runtime runs lacked the local `DYLD_LIBRARY_PATH` and
failed, matching the already documented native linker limitation. Both pass with
`DYLD_LIBRARY_PATH=/opt/homebrew/lib`; no production or test assertions were changed.
The environment's pnpm fallback attempted installation instead of exec; its moved
existing dependencies were restored, and checks ran directly with the installed
Node binaries. Rust formatting and whitespace checks pass.

Receipts: `/tmp/g5-pr268-rebase-range-diff.txt`, `-api.log`, `-cli.log`,
`-types.log`, `-frontend.log`, `-terminal-configured.log` and
`-runtime-configured.log`. Remote CI must qualify the rewritten head; prior-head
CI success is not claimed as evidence for this head.

## Rebase 20df7d80 approval authority

The follow-up observed Rust CI success at `4afebabc`, E2E still running, and no
unresolved review threads. Main advanced through PR #280 and made the PR conflict.
Before replay, incremental self-review compared main's addressed-human decision and
organization-admin requirement contracts with this PR's typed publication admission,
read authority and version-bound approval. These are distinct grants; no PR guard
may be relaxed on account of approval administration.

The sole conflict was `g5.architecture.governance_owner`'s implementation footer:
main correctly removes the old nonowner-approval gap, while this PR adds read/write
implementation evidence. Both are retained. The adjacent PR-flow footer's stale
migration range becomes "Main's migration directory is unchanged." This is metadata
reconciliation; no contract or product-code repair was indicated. Range-diff confirms
all implementation patches are unchanged. Main's route scope audit and this PR's
shared-create service audit both remain present. Initial discovery found no actionable
defect; combined approval/PR API regressions and the approval UI suite qualify the
composition below. The prior local CLI/runtime evidence remains valid because none
of those files changed. No live deployment or dev-stack restart is claimed.

Verification passes: 91 API/static tests (13 PR reads/checks, 18 approval, 50 PR
publication/recovery and 10 route-authority cases), plus all 40 approval UI cases.
The approval suite exercises org-admin-only tokens, workspace-scoped grant refusal,
nonowner approval/rejection, current read access and requester-preserving chains.
The PR suites retain source-owner/current-incarnation checks and version evidence.
Rust formatting, whitespace and paired ontology metadata checks pass. Every one of
the four PR commits has a migration directory identical to main. No behavior was
changed to satisfy tests. Receipts: `/tmp/g5-pr268-rebase-20df-api.log`,
`/tmp/g5-pr268-rebase-20df-ui.log` and `/tmp/g5-pr268-rebase-20df-range.txt`.
Remote CI on the rewritten head remains pending.
