# Independent PR retrieval discovery review 2

Reviewed the uncommitted CLI PR retrieval amendment against HEAD `75b522e7` on
2026-09-30. This is an independent, full-scope, read-only discovery review. No
implementation-session explanation or peer report was read. Production code, tests and
ontology were not edited; this report is the only review artifact written.

Started with the saved plan's `Ontology changes and governance`, then `request.md`,
the CLI amendment, the actual ontology diff and current governing blocks/footer entries.
The actual incremental ontology addition is `g5.governance.pr.read`; the plan also
preserves previously committed authorship and shared-connection changes. Those earlier
changes were reviewed only where the new read path reuses their dependencies.

## Initial findings

### R2-1 — P1: VM PR commands cannot pass outer authentication

**Location:** `backend/g5-api-routes/src/routes/internal.rs:200` (new route);
`backend/g5-api-core/src/middleware/auth_middleware.rs:490–511,701–770` (unchanged
admission owner); `backend/g5d-cli/src/pr.rs:35–39,67–68` (consumer).

The new `/api/internal/workspaces/:id/pull-requests/info` route is absent from
`is_workspace_incarnation_internal_route`. The middleware only validates an incarnation
token for explicitly admitted method/path shapes; otherwise this path is not a host-only
path and immediately returns Unauthorized. Consequently every new `pr` command from a
Provided-Agent cwd or ordinary workspace VM takes the internal URL and fails before
`read_internal` or its current-incarnation checks execute. Registering `exempt_route`
does not admit it through the outer middleware.

This violates the current-workspace command outcome in `g5.governance.pr.read` and the
allowed current-token case in `g5.security.admission`.

**Proposed repair:** Admit exactly GET plus this path and a valid workspace UUID in the
incarnation middleware; retain denial for other methods, malformed IDs, host tokens and
human tokens. Extend the allowlist unit test alongside the route change.

**Discriminating check:** Run the existing
`stored_reads_enforce_visibility_and_current_workspace_incarnation` integration test
through the real app middleware. Its live incarnation request must be 200 before the
cross-workspace and expired-lease requests are checked. The CLI wiremock tests cannot
detect this because their server always directly accepts the internal URL. Add a live
GitHub selector with a current VM token if claiming the entire provider path is exercised.

**Evidence:** Direct source tracing plus an executed source check confirms
`pull-requests` appears in neither admission function. No API runtime check was run by
this reviewer; parent-owned API tests were in progress.

### R2-2 — P2: The new adapter fails the repository's route scope guard

**Location:** `backend/g5-api-routes/src/routes/pull_request_reads.rs:1–18` and
`backend/g5-api-routes/src/routes/guards.rs:204–254`.

`all_route_files_enforce_scopes` scans every nonexempt route source file for an
`AuditedRouter`, `audited_route`, `RequireScopes` or `route_layer` reference. The new
`pull_request_reads.rs` file contains none and is not an exempt or infrastructure file,
so this required test fails even though the routes are actually registered with the
right public scope in `workspaces.rs` and an internal exemption in `internal.rs`.
This is a CI integration defect, not an unscoped-public-route claim.

**Proposed repair:** Keep this helper adapter compatible with the structural guard by
using the repository's intended route organization, or explicitly teaching the guard
about handler-only files whose registrations it verifies elsewhere. Do not add an
inert matching comment merely to satisfy the textual scan.

**Discriminating check:** Run
`routes::guards::tests::all_route_files_enforce_scopes`. If changing guard logic, show it
still rejects a truly unregistered/unscoped route while permitting this split adapter.

**Evidence:** Executed the exact string-presence predicate on the new file; result was
false. No Cargo test was started by this reviewer.

### R2-3 — P2: Explicit local workspace selection can silently read a different workspace

**Location:** `backend/g5d-cli/src/pr.rs:17–35`.

Although `--workspace PATH` is resolved and its config is loaded, the selected workspace
ID always prefers discovered stable metadata and then `G5D_WORKSPACE_ID` over that
explicit path's config. Thus from current workspace A, `g5d --workspace /checkout-B pr
list` can return A's PRs successfully while ignoring the explicitly selected B. The
documented path override says it selects the local workspace directory, and the existing
stable-context helper explicitly recognizes an outside classic workspace override.
The new command does not honor that exception. `--workspace-id` happens to work because
it has a separate highest-priority branch.

**Proposed repair:** Distinguish explicit checkout selection from implicit discovery and
make it select that checkout's workspace ID. Keep credential scope intact: if the ambient
VM token cannot access B, fail rather than silently reading A. Reuse the established
outside-classic-workspace handling where appropriate instead of introducing another
selection precedence rule.

**Discriminating check:** Create isolated A stable metadata (or A's environment ID) and
a B `.g5/workspace.yaml`; invoke with `--workspace B`. Assert that the request targets B
or is explicitly rejected for the VM credential mismatch, never that it succeeds against
A. Also exercise plain cwd config discovery, explicit `--workspace-id`, and a stable cwd
with its correct current token. Existing CLI tests cover only environment discovery and
an explicit UUID outside a checkout.

**Evidence:** Source inspection; no current executable was available for this check.
`target/g5-dev/g5d pr --help` uses an older binary without `pr`; `target/debug/g5d` could
not start because `libkrun.1.dylib` was unavailable. No build was started.

## Commitment and scope accounting

| Commitment | Actual path and inspected counterexample/check | Result or gap |
|---|---|---|
| `g5.governance.pr.read`: workspace defaults, list/view/status, readable/JSON | `PrArgs`, dispatch, `pr::execute/request_path/select_output/render`; env, config, stable metadata and explicit selectors; empty/missing results | Source implements the intended shape and labels stored state; R2-1 blocks VM use and R2-3 breaks the explicit-path variation. CLI tests inspected, not executed. |
| `g5.governance.pr.read`: attached repository and ambiguity | `workspace_pull_request_read::selection/read_github_pull_request`; Git URL identity and selected attached repository; positive number, conflicting URL/repo, multiple repos, foreign host and redirect | Source preserves the boundary. Tests cover number/URL, mismatches, ambiguity, unattached and uncovered repos. |
| `g5.governance.pr.read`: honest provider information | Field mapping, null passthrough, explicit `source`, comments count and `commentsHasMore`; malformed provider response and non-success HTTP handling | Source preserves unknown nullable fields and distinguishes live/stored state. Open and merged cases appear in tests; malformed JSON/shape and closed-unmerged runtime evidence are absent. |
| `g5.governance.pr.read`: no PR/publication/compute mutation | Both read adapters; `list_workspace_pull_requests/list_prs/attach_provider_repos`; separate provider read implementation | Source contains only durable reads and provider GETs after permitted credential mint/refresh. It does not reuse mutation-producing refresh/reconciliation. Provider test checks workspace row equality and no PR insertion; other persisted PR/provider rows are not seeded for mutation checks. |
| `g5.security.read`, `read.durable` | `read_public` → `scoped_db_for_workspace` → workspace access helper; durable list and provider GET, no VM file read | Correct source composition. API tests inspect allowed org reader, hidden workspace and outsider refusal. Invitation/admin cases remain unexecuted here. |
| `g5.security.admission`, service identity boundary | Outer auth middleware → `WorkspaceIncarnationContext` → `scoped_db_for_workspace_incarnation` → live owner/org/VM/epoch/lease checks | R2-1 prevents the allowed case. Handler retains cross-workspace/current-incarnation checks. Tests include lease expiry; owner transfer, replaced VM/epoch and wrong token class are not covered in this new suite. |
| `g5.security.tenant`, `tenant.rls`, `tenant.write` | Scoped repo mapping query; credential owner rechecks actor membership and repository URL; no write action introduced | Source preserves scoped organization and workspace resolution. API tests use repository TestApp helpers claiming restricted roles; this reviewer did not independently execute/inspect database role grants. |
| `g5.security.credentials.publication`, redaction | `RepositoryCredentials::resolve(ReadPullRequests)`; App `PullRequestsRead` narrow mint, personal reader SCM fallback, fixed configured API destination, redirects disabled, errors omit provider body | Source keeps tokens server-side. Existing creation/push/merge operation selection is unchanged. Fake-provider tests check actor personal token, App read token, failure-body secrecy and no redirect following. External GitHub permission matrix remains unverified. |
| `g5.governance.publish.github-author`, `publish.actor`, `publish.readiness` | Existing credential enum and distinct creation operation; new read only requests `ReadPullRequests` | No changed creation/push/readiness behavior found; prior live-authorship gaps are not reopened as defects in this amendment. |
| `g5.architecture.governance_owner`, `g5.design.change-locality`, `minimal-mechanism`, `concerns`, `reuse`, `policy` | CLI owns parsing/presentation; shared public/internal read adapters; provider service selects attached repository and credentials; helper reuse checked for effects | New bounded read operation is justified because existing provider refresh mutates state. No schema, parallel token store, generic mutation API or unnecessary background mechanism. R2-2 is the route organization/guard compatibility gap. |
| `g5.quality.review.cases`, `g5.quality.evidence`, `evidence.claims` | All new unit/API/CLI tests and affected auth/credential/read owners read | Tests are meaningful at their stated boundaries. The mock CLI transport does not prove middleware admission. This report separates source checks from unexecuted runtime evidence. |

All changed tracked source/docs and the new reader, adapters, CLI command, API tests,
CLI tests and command documentation were inspected. Both agent-guide copies make the
same relevant command additions. No unrelated pre-existing debt is requested for repair.

## Verification limits and proposed metadata update

No Cargo build/test ran because the parent was executing tests. Executed only read-only
source checks and help probes of existing binaries. The route and CI failures above are
direct consequences of the inspected source; the explicit checkout finding is source
evidence only. No live GitHub, live OAuth, network credential flow, or database test was
performed by this reviewer.

After consolidation and repairs, update the paired `g5.governance.pr.read` footer from
“Planned” with actual implementing owners and the exact completed checks, retaining any
unexecuted GitHub/current-VM/default-discovery cases. The new read operation can be added
to the Git/governance module's public-operation navigation where the main agent judges
it necessary; this review found no need to change the governing read meaning.
