# Independent PR retrieval discovery review — three

Status: independent read-only discovery complete. No production,
test or ontology changes; no peer reports or implementation explanation read. Review
scope is the uncommitted CLI retrieval amendment over HEAD `75b522e7`, including all
changed files and new source/tests/docs. Prior authorship/connection work is context
only, except its credential operation is a dependency of this amendment. The initial
finding below was saved before sharing with the coordinating agent and remains intact.

## Initial actionable findings

### R3-1 — P1: Register the internal PR read in incarnation authentication

The new route at `backend/g5-api-routes/src/routes/internal.rs:200–205` cannot be
reached by its intended caller. `g5d-cli/src/pr.rs:35–39` uses the internal endpoint
whenever stable workspace metadata or `G5D_WORKSPACE_TOKEN` exists. However,
`g5-api-core/src/middleware/auth_middleware.rs:490–511` admits incarnation tokens only
for `is_workspace_incarnation_internal_route`, and that exact method/path allowlist
at lines 701–784 contains no `GET /api/internal/workspaces/:id/pull-requests/info`.
The path is also not host-only, so middleware returns Unauthorized before the new
handler can check the valid current owner, VM, epoch and lease. This breaks all three
PR commands in workspace VMs, including the Provided-Agent default workflow.

Governing commitments: `g5.governance.pr.read`, `g5.security.admission` and
`g5.design.enforcement`. Recommended repair: add only this exact GET path with a valid
workspace UUID to the incarnation allowlist; preserve host/user-token separation and
the handler's current-incarnation validation.

Discriminating verification: the existing
`stored_reads_enforce_visibility_and_current_workspace_incarnation` API integration
test must reach its valid-incarnation 200 assertion through the full application
middleware. Add the new allowed route plus POST, malformed UUID, ordinary-user JWT,
stale epoch and expired lease refusal cases to middleware/API coverage. The current
CLI wiremock tests cannot detect this omission because they replace server auth.
Evidence here is direct source tracing; no runtime build/test was run by this reviewer.

## Review inventory and commitment ledger

The first plan section, saved request, complete retrieval amendment, actual tracked
ontology diff and current relevant footer entries were read first. The incremental
ontology change is `g5.governance.pr.read`; prior authorship/connection commitments
are retained governing context, not newly re-reviewed work. Inspected all 14 tracked
changed files plus the six new source/test/document files in the supplied snapshot.
Followed the affected consumers into authentication middleware, workspace access and
incarnation helpers, repository credentials and App token selection, existing stored PR
queries/DTOs, CLI authentication/configuration/discovery, and the integration harness.

| Commitment / outcome | Actual path and discriminating case | Inspection result or gap |
|---|---|---|
| `g5.governance.pr.read`: current workspace, override, list/view/status | `g5d-cli/src/{lib,dispatch,pr,stable_context}.rs`; stable metadata → environment → workspace config; explicit UUID override; latest open outgoing selection and list state/limit | Command/selection implementation agrees with saved design. VM transport is broken by R3-1. Source/unit/CLI test definitions inspected; none executed here. |
| `g5.governance.pr.read`: scoped live number/URL | `workspace_pull_request_read.rs::selection` and attached `workspace_repos` join; ambiguous number, mismatched repository, foreign-host/credential-bearing URL versus valid number/URL | Provider requests derive from a validated attached Git URL, not the selector URL or imported display name. Explicit mismatch and ambiguity fail before token requests. Route integration test definitions cover these boundaries. |
| `g5.governance.pr.read`: stored/live honesty, readable/JSON output and unavailable data | `pull_request_reads.rs::read`, `pr.rs::{select_output,render}` and live response projection; null mergeability and provider 403 versus valid detail | Separate `source` values, stored-state label, preserved null fields, bounded first-page comments with `commentsHasMore`, error status without provider response body. Terminal controls are stripped from human output. No additional defect found. |
| `g5.governance.pr.read`: no publication, PR mutation, merge or compute | Reused `list_workspace_pull_requests` → `list_prs`/`attach_provider_repos` is SELECT-only; new live reader performs GET calls after credential resolution | No mutating lifecycle reconciliation or compute path is invoked. Credential refresh/minting remains the explicit permitted effect. Provider fake and persisted-state assertions exist; runtime verification is unexecuted here. |
| `g5.security.read`, `g5.security.read.durable` | `read_public` → `scoped_db_for_workspace` → `enforce_workspace_access_for_user`; durable PR rows or provider GitHub read | Public reads retain owner/admin/member visibility rules and read scope; no owner-only mutation guard was substituted. Org reader success, private outsider refusal and incoming PR read are in test definitions. |
| `g5.security.tenant`, `.tenant.rls`, `.tenant.write` | Scoped workspace resolution; joined org identities; credential owner separately revalidates actor membership and captured repository URL | Tenant transaction boundaries and server-owned credential authority are preserved. Integration harness configures distinct restricted runtime roles. No writes were added to membership-only grants. |
| `g5.security.admission`, `.login.service-identities`, `g5.design.enforcement` | Authentication middleware → `read_internal` → `scoped_db_for_workspace_incarnation` | R3-1 prevents legitimate VM reads before handler admission. Once registered, existing helper validates workspace, organization, current owner, VM, epoch, status and lease. Host/user-token separation and negative method tests should accompany repair. |
| `g5.security.credentials.publication`, `.credentials.redaction` | `RepositoryCredentials::resolve(ReadPullRequests)`; App coverage/capability; personal SCM fallback without App binding; `get` uses no redirects and redacted errors | Tokens remain server-side. App read operation remains installation-scoped and narrow; personal read uses the requesting user's connection. Existing creation/authorship policy is untouched. Failure and redirect test definitions cover the new GET owner, but no live provider execution was performed. |
| `g5.governance.publish.github-author`, `.publish.actor`, `.publish.readiness` | Existing operation mapping and provider lifecycle remain unchanged except adding the read child module/export | Read operation does not reuse or modify new-PR creation, push, readiness or merge paths. No regression identified by source inspection; prior runtime evidence was not rerun or claimed. |
| `g5.architecture.governance_owner`, `g5.design.{minimal-mechanism,change-locality,concerns,reuse}`, `g5.architecture.policy` | CLI presentation, shared route adapter, provider reader child module and existing credential/storage owners | New mechanism is narrow and justified by avoiding the mutating provider refresh path. Both route variants converge on the same read operation. No schema, token store, background sync or unrelated abstraction added. |
| `g5.quality.review.cases`, `.evidence.claims` | New provider selector unit tests, three CLI black-box tests and four real-route integration tests | Definitions exercise useful permitted/refused variants. They are not evidence of passes until executed. The API test for valid current incarnation should directly catch R3-1. |

## Additional verification gaps, not separate code findings

- The black-box CLI tests use an environment workspace ID or explicit UUID; they do
  not exercise actual `.g5/workspace.yaml` or Provided-Agent metadata discovery. A
  scratch-workspace invocation with those real inputs would substantiate the central
  default-workspace promise, including an explicit `--workspace PATH` selection.
- The internal API test covers expired lease and mismatched workspace, but the same
  route should also discriminate stale VM ID/epoch, changed owner, wrong token type,
  missing token and invalid method. Source reuses the appropriate current-incarnation
  helper, so these are evidence gaps rather than independent guard defects.
- Invalid successful provider JSON, timeout and comment-fetch failure branches are
  inspected but lack route-level execution evidence in this review. Live GitHub
  permissions, real comments pagination and GitHub availability remain unverified.

## Proposed metadata reconciliation

After coherent repair and actual checks, replace the `g5.governance.pr.read` footer's
"Planned" assessment with the implemented CLI/read-owner paths, the actual route/CLI
results and the remaining live-provider limit. The saved amendment describes the
intended operation accurately; R3-1 requires code repair rather than weakening the
contract. No unrelated ontology rewrite is warranted by this discovery.

## Verification limits and effort

`git diff HEAD --check` completed without findings. No Cargo builds or application
tests were started, as requested while the main agent runs checks. This is one fresh
reviewer context performing a full-scope source review; no further delegation and no
measured runtime or token-cost comparison. One actionable P1 finding is recorded;
implementation and central VM-path verification remain incomplete until R3-1 is fixed
and the full application-path checks run successfully.
