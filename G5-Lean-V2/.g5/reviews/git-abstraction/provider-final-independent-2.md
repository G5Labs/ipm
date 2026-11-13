# Final provider migration — independent discovery 2

Independent, read-only full-scope review of the frozen final migration against `2433688c`. No author narrative, other reviewers’ findings, or evaluator feedback was supplied before this report. The original request, pinned plan beginning with “Ontology changes and governance,” its embedded ontology diff, relevant current governing contracts and their metadata, all 30 snapshot files, the tracked patch, and material unchanged consumers were inspected. No production/ontology repair or delegation was performed. This report is saved before findings are shared.

## Scope and frozen evidence

- Snapshot: `/tmp/g5-provider-final-review/snapshot.tar`, SHA256 `fd795bc7cef26fd21401dc667d3e5b4e9cb9ca05a2edbcfd8119b7cd7ea873b6`.
- Tracked patch: `/tmp/g5-provider-final-review/tracked.patch`, SHA256 `f9b40300a5b0fa80259746d1b5b6e31d6044a440d64905b24a514c9b73eb82a1`.
- Extraction and investigative resources are isolated at `/tmp/g5-provider-final-review/reviewer2`. Byte comparison of every inventory file against the working tree reported no mismatches.
- The 30-file increment changes provider implementation and guards; it does not change ontology body commitments. The design explicitly defers the final metadata assessment until discovery and consolidation. Its older native/App/issue implementation receipts are retained historical evidence, not final-source verification of these newly migrated consumers.
- The approved retry constraint remains authoritative: no added tracking tables, retry state, queues, workers or transport replay. Existing PR 422 reconciliation, SCM token refresh and platform-report outbox recovery remain with their owners. The branch/object confidentiality extension remains parked.

## Findings (unrepaired discovery)

### R2-1 — P2: Normalize a successful-HTTP merge refusal before exposing its message

**Path:** `backend/g5-scm/src/providers/pulls.rs:256–280`, consumed by `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:1721–1725`.

`merge_pull_request` returns the wire `GithubMergeResponse` directly for any successful HTTP response. When its `merged` field is false, the unchanged lifecycle owner puts `message` directly into a public `G5Error::Conflict`. That path bypasses the shared redaction/error machinery, loses operation/target/effect/request/rate evidence and can expose the selected grant in a provider-supplied refusal. The new typed operation therefore has not closed the error boundary promised by `g5.architecture.git_outcomes`, `g5.architecture.scm_provider_client` and `g5.security.credentials.redaction`.

A fresh public-client loopback probe with HTTP 200 and `{ "merged": false, "message": "refused selected-token-canary" }` returned `Ok(GithubMergeResponse { ... message: Some("refused selected-token-canary") })`. The owner’s direct public error projection is source-verified. This is an inherited consumer behavior exposed by the new migration, not a claim that the refactor newly introduced the original unchecked string.

**Proposed repair:** interpret the provider’s negative merge result inside the typed protocol client; sanitize the diagnostic while response and selected-secret evidence are still available, and return an appropriately typed semantic refusal/error. Alternatively normalize the returned diagnostic there while preserving the error evidence in a dedicated owned outcome. Do not introduce replay. Add a public-client canary test and an owner/API assertion that the public error has no selected token and retains useful operation/outcome guidance. Keep a valid merged response control.

### R2-2 — P2: Bind report issue responses to the selected repository and issue number

**Path:** `backend/g5-scm/src/providers/reports.rs:107–120`; `create_issue` and `find_issue_by_marker` both consume it.

The new report validator accepts any positive number and any URL under the configured web origin/prefix. It does not verify the configured destination repository, the `issues` resource, or agreement with the returned number. A public-client probe targeting `owner/reports`, number 77, returned `Ok` for `https://reports.test/git/other/repo/pull/5`. The outbox owner subsequently treats the returned number/link as completed report delivery. The operation’s explicit single-destination grant and the plan’s accepted-URL validation are not enforced on this result. This is an invalid-provider-response boundary gap, not evidence of a live provider returning such a response.

**Proposed repair:** validate or construct the canonical issue URL using the selected report repository and returned positive issue number, preserving the configured web prefix. Reject a different repository/resource/number as an invalid response with the already confirmed write effect retained. Test both create and marker-recovery reads, with a correct Enterprise-prefix URL as the permitted control. This applies `g5.architecture.github_transport`, `g5.architecture.report_owner`, `g5.architecture.scm_credential_scopes` and the pinned plan’s URL-validation requirement.

### R2-3 — P2: Repair inconsistent PR fixtures and finish lifecycle verification

**Path:** `backend/g5-api/tests/api_workspace_pull_requests.rs`, representative fixture at line 2898; related response fixtures at lines 3176, 3747, 3928, 3982, 4056 and 4156.

The frozen current-source lifecycle run ends with **45 passed / 5 failed**. Its provider fakes send `https://github.com/acme/api/pull/...` even when the authenticated request and current selected Git URL identify `g5-test-<org>/api`. The new PR identity check correctly rejects the inconsistent response. Some tests fail immediately with “invalid pull request identity or URL”; others fail later because the refusal prevents provider identity persistence/refresh/merge. These failures leave title retry, automatic refresh, terminal-pin and unconfirmed-cut guarantees unverified for this migration.

Failing tests:

- `automatic_pr_refresh_checks_identity_included_members_and_terminal_pins`
- `automatic_pr_refresh_discovers_missing_provider_after_local_refresh`
- `automatic_pr_refresh_never_repins_an_unconfirmed_remote_cut`
- `automatic_pr_refresh_waits_for_remote_push_recovers_and_keeps_merge_checks`
- `explicit_title_retries_preserve_identity_and_update_included_members`

**Proposed repair:** make successful fixtures return URLs matching the repository actually selected/requested and update the corresponding successful URL assertions. Preserve dedicated mismatched-identity refusals. Rerun all five and the relevant lifecycle suite; do not weaken the new production validation or accept the previously green historical suite as proof. Governing obligations: `g5.architecture.pr_flow`, `g5.governance.pr.follow-push`, `g5.governance.publish.github-author`, and `g5.quality.review.producer` / `.cases` / `g5.quality.evidence.claims`.

## Commitment ledger

The entries below distinguish source inspection, actual execution, and retained limits.

| Commitment(s) | Actual path and discriminating case | Result / gap |
|---|---|---|
| `g5.architecture.scm_provider_client` | AuthService, ScmTokenManager, PR lifecycle/read/checks and platform report wrappers now delegate protocol to the SCM provider modules; no migrated owner holds raw HTTP. Account and repository grant capabilities are separate. | Source-inspected, provider/API tests pass except the lifecycle failures; semantic merge refusal remains R2-1. |
| `g5.architecture.scm_credential_scopes` | Candidate/account constructors require no repository; repository capabilities reject prohibited operations before HTTP; `RepositoryCredentials` retains initiating-user creation and installation-only/read policies. App/report token issuance consumes owner-selected grants. | Preserved by source and supplied public-client/owner tests. No actor fallback was added. R2-2 concerns returned destination identity. |
| `g5.architecture.github_transport` | One private `GithubHttp`, independently configured login/repository/report endpoint inputs, owner prefixes, no redirects, request/body deadlines and 2 MiB cap. PR selectors reject foreign origins and malformed selectors; check links are references only and are never fetched. | Provider suite passes. Report validation has R2-2. Live SaaS/Enterprise interaction not executed. |
| `g5.architecture.git_outcomes` | Status-aware `Response::failure` retains known write confirmation as Partial, missing acknowledgement as Unknown, refused 4xx as Refused, request and retry evidence, sanitized messages. PR wrapper preserves message evidence and blocks rate-limited 422 fallback. | Public-client tests cover invalid/oversized/unknown writes and rate evidence; R2-1 is an unclosed semantic success-body error path. |
| `g5.architecture.git_retry` and `g5.architecture.recovery` | `.retry(reqwest::retry::never())`, no internal transport loop; caller-owned PR 422 legacy reconciliation, refresh/cache and report outbox are retained. Diff adds no schema/state worker. | Preserved by inspection and single-request fixture expectations. No new generic recovery mechanism. |
| `g5.architecture.identity_owner`, `g5.security.login`, `.login.scm`, `.login.service-identities` | Live OAuth callback/exchange/user/email path uses LoginClient; SCM/App OAuth uses ConnectionOAuthClient. Verified-email selection/linking stays in AuthService. PAT/account operations use their SCM grant. | Current frozen `api_auth`: 46 pass, including callback, verified secondary email, PAT, GitLab connection and configured-origin tests. Actual browser/provider login and revocation not executed. |
| `g5.security.credentials.publication`, `.credentials.redaction`, `g5.architecture.git_credentials` | Credential owner still selects server-side tokens; Secret/OAuthTokens Debug redacts; transport scrubs selected credentials; checks additionally scrub effective configured secrets before output. No guest token propagation changes. | Source and unit/API evidence preserve separation; R2-1 exposes a selected secret through negative merge-message projection. Earlier native injection receipts are historical, not rerun here. |
| `g5.architecture.pr_flow`, `g5.governance.publish.github-author`, `.publish.actor`, `.pr.follow-push` | Existing lifecycle SQL/authority/credential ordering remains; creation uses initiating-user grant; unknown writes surface to caller without fresh transport replay. | 45 lifecycle cases pass; five intended guarantees remain blocked by R2-3 fixtures. No production regression is inferred merely from those fixture failures. |
| `g5.architecture.pr_checks`, `g5.governance.pr.checks`, `.checks.revision`, `.checks.details` | Authorized attached repository selection, read grants, exact requested head, test-merge snapshots, bounded pagination/annotations, same-revision/rerun consistency and stop-fanout are retained in typed protocol/owner split. | Frozen PR-read suite: 13 pass. Source preserves no-publication/CI-mutation semantics. Live provider scheduling completeness cannot be proven by reads. |
| `g5.architecture.report_owner`, `g5.quality.platform-reports`, `.identity`, `.quota` | Report JWT/cache/outbox/quota remain in API owner; SCM narrows installation request and uses independent configured web/API roots for deterministic screenshots and issue calls. | Frozen report recovery suite: 9 pass. Current destination-result validation gap is R2-2. Real GitHub delivery not executed. |
| `g5.architecture.git_guard`, `g5.quality.review.guards`, `.cases` | Whole-backend source traversal rejects raw reqwest outside named non-Git owner operations; migrated targets are required, existing reader/native/backend checks retained; violating and benign fixtures present. | 28 current-source guards pass. Structural limits remain: this is a reqwest/syntax ownership guard, not a complete Rust type/macro/dataflow proof. No explicit provider-name-dispatch guard is present; current protocol selection is concentrated in composition, while legacy domain eligibility/refresh policy still inspects provider identity. Do not claim universal provider-dispatch enforcement. |
| `g5.architecture.dependencies`, `.compatibility`, `g5.design.minimal-api`, `.information-hiding`, `.abstraction`, `.enforcement`, `.change-locality` | SCM providers depend downward, expose grants/typed operations instead of Client/RequestBuilder; providers feature adds base64 without native/libgit2 requirements. Existing aliases/DTO projections avoid duplicate API policy. | Source and provider-only tree/build receipts support closure. No unjustified new persistence/service layer found. Report and merge result validation are the bounded gaps above. |
| `g5.architecture.commands`, `g5.design.transaction-composition`, `.adapters`, `.reuse` | Existing AuthService, RepositoryCredentials, PR owner, report outbox own database and orchestration; SCM contains protocol/normalization only. | Preserved in inspected owners. Moving HTTP did not move SQL or authority into the transport. |
| `g5.architecture.git_execution`, `.git_ref_updates`, `.git_native_runner`, `.git_locations` | This increment changes provider transport and structural guard coverage, not native runner/ref/layout implementations. Public interfaces and backend guards retain the earlier closed native boundaries. | Earlier committed evidence is retained as historical; no fresh native runtime/ref/layout claim is made here. |
| `g5.architecture.content_reader`, `.projection_owner`, `.workspace_registry`, `g5.data.git-authority` | Authorized reader and owner guards remain; provider reads select the authorized attached repository and do not replace content-reader authority. | No new bypass found. Earlier content/projection/bootstrap tests are not final-source reruns by this reviewer. |
| `g5.git.network.cas`, `.network.ref-scope`, `.receipt.write-admission`, `g5.governance.publish.proof` | Existing ref admission, publication and durability implementation is unchanged in this provider diff; lifecycle still delegates those operations. | Preserved by source-scope comparison; broader native concurrency/proof limits from prior metadata remain. |
| `g5.runtime.drain`, `.materialization`, `.bundle`, `.workspace-cli`, `.workspace-cli.context`, `g5.git.checkpoint.selected`, `.checkpoint.isolation`, `.checkpoint.retry`, `.durability.working-state`, `.flush.content`, `.exclusions`, `g5.scope.capacity-recovery.gc` | No dependent runtime/materialization/checkpoint/flush/GC implementation change in this frozen increment. Whole-backend dependency/native guards remain active. | No fresh runtime verification of those prior increments; retained earlier evidence and its VM/interruption/cancellation limits are not silently upgraded. |
| `g5.security.mvp-exposure.objects` | No new object/branch confidentiality mechanism appears in snapshot. | Parked extension remains out of scope, as instructed. |

## Verification receipts and freshness

The main agent supplied test execution receipts only. No author or peer findings were consumed. Older `/tmp/g5-provider-pr-reads.log` is a pre-final 11-pass/2-fail receipt, superseded for this snapshot by the rebuilt frozen 13-pass run; the newer source has the relevant rate-detection and public-web fixture changes. It is not counted as a current defect or pass.

| Check | Evidence | Result |
|---|---|---|
| Provider-only unit suite | `/tmp/g5-provider-frozen-tests.log`; compilation occurs after the snapshot | 32 pass / 0 fail / 2 filtered |
| Whole source/manifest/native/provider guard suite | `/tmp/g5-provider-final-guard.log`; guard source unchanged since that run and identical to snapshot | 28 pass |
| API binary rebuild | `/tmp/g5-provider-final-owner-build.log` | All four API binaries rebuilt after snapshot; compiled artifact is newer than all migrated provider sources |
| Auth/login/account owner suite | `/tmp/g5-provider-frozen-auth-owner.log` | 46 pass |
| Platform report outbox suite | `/tmp/g5-provider-frozen-report-owner.log` | 9 pass |
| PR lookup/check suite | `/tmp/g5-provider-frozen-pr-reads.log` | 13 pass |
| PR lifecycle suite | `/tmp/g5-provider-frozen-pr-lifecycle.log` | 45 pass / 5 fail (R2-3) |
| Independent public-client probes | `/tmp/g5-provider-final-review/reviewer2/probe.rs`, `probe.log` | Reproduced R2-1 and R2-2 via real loopback HTTP and public provider methods |

Probe freshness: the probe was compiled with `rustc` against `target/debug/deps/libg5_scm-cb95f83459e19a0f.rlib` (20:29 build), whose dep-info lists all current provider modules; the latest provider source was 20:28:17. Every scoped source was compared byte-for-byte to the frozen tar. Its matching Tokio dependency was used. The probe writes/builds only beneath reviewer2 and does not alter shared Cargo artifacts. An initial local-listener sandbox refusal was resolved by an approved loopback-only tool escalation using synthetic fixture credentials. No external provider requests or database mutations were made by the reviewer. The two observed public-client results are runtime evidence; the subsequent owner/outbox projections are source-inspection evidence.

## Consolidation / metadata recommendations

Retain the raw findings above and have the main owner adjudicate them before a single repair. Final provider metadata should link current provider paths and evidence, record any unresolved semantic merge/destination validation gap directly, and preserve the failed lifecycle cases as unverified until repaired and rerun. Refresh the scoped platform identity/report and PR read/lifecycle assessments where the new current-source evidence changes them; do not convert older native implementation receipts into blanket final-branch verification. The existing design is sufficient for these repairs; no contract expansion, retry amendment or new state is needed.

Implementation and verification are separate: the protocol migration is substantially in place and no retry/scope inflation was found, but this frozen delivery is not fully verified because the lifecycle suite is red and the two bounded client counterexamples remain unrepaired.

## File coverage

- `.g5/ontology-plan-git-abstraction.md`
- `backend/g5-api-core/src/auth/mod.rs`
- `backend/g5-api-core/src/auth/oauth.rs`
- `backend/g5-api-core/src/auth/scm.rs`
- `backend/g5-api-core/src/maintenance/mod.rs`
- `backend/g5-api-core/src/services/auth_service.rs`
- `backend/g5-api-core/src/services/config_service.rs`
- `backend/g5-api-core/src/services/github_app.rs`
- `backend/g5-api-core/src/services/github_app_binding.rs`
- `backend/g5-api-core/src/services/mod.rs`
- `backend/g5-api-core/src/services/platform_issue_github.rs`
- `backend/g5-api-core/src/services/workspace_pull_request_checks.rs`
- `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs`
- `backend/g5-api-core/src/services/workspace_pull_request_read.rs`
- `backend/g5-api-foundation/src/config.rs`
- `backend/g5-api/tests/api_pull_request_reads.rs`
- `backend/g5-api/tests/api_workspace_pull_requests.rs`
- `backend/g5-api/tests/authoritative_workspace_reader_guard.rs`
- `backend/g5-api/tests/helpers/mod.rs`
- `backend/g5-api/tests/platform_issue_delivery_recovery.rs`
- `backend/g5-scm/Cargo.toml`
- `backend/g5-scm/src/providers/account.rs`
- `backend/g5-scm/src/providers/app.rs`
- `backend/g5-scm/src/providers/checks.rs`
- `backend/g5-scm/src/providers/login.rs`
- `backend/g5-scm/src/providers/mod.rs`
- `backend/g5-scm/src/providers/pulls.rs`
- `backend/g5-scm/src/providers/reports.rs`
- `backend/g5-scm/src/providers/tests.rs`
- `backend/g5-scm/src/providers/transport.rs`
