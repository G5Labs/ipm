# PR checks: independent full-scope review one

Review mode: independent, read-only discovery. Scope is the working-tree change above
`fa1685cca72ebc548ab4821db95fd60ab3fe72ba`, including the new
`backend/g5-api-core/src/services/workspace_pull_request_checks.rs`. The unrelated
`docs/ui-workspace-agent-parity-audit.md` is excluded. Production, tests and ontology were
not changed. No implementation explanation or current peer review was read before this
report was saved. No further delegation was used.

Inputs: `AGENTS.md`, the ontology-review skill and design-review guide, the original
request, the current first amendment in `ontology-plan.md`, the actual ontology delta,
and the governing blocks/metadata listed below. The first amendment's ontology diff
matches the four added commitments and architecture operation row in this change.

## Findings saved before repair

### F1 · P2 · A requested rerun can still return the old successful run

Affected path: `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:288–295`
and `:374–383`.

The read enumerates check runs with `filter=latest` and treats their conclusions as the
whole current outcome. It never reads check-suite state. GitHub documents that requesting
a check rerun resets the containing suite to `queued` and clears its conclusion **without
updating the check run itself**. An old successful run therefore remains a pass here while
its replacement is pending; the final head read sees the same SHA and does not invalidate
it. `g5d pr checks --watch --head <same SHA>` exits 0 in that interval.

This violates the saved design's requirement that a rerun invalidates its earlier result
and the current-attempt/revision obligations of `g5.governance.pr.checks` and `.revision`.
The observed-only/merge-approval disclaimer does not cure reporting the superseded attempt
as current. Read sufficient suite/attempt state through the provider owner and represent
an outstanding rerequest as pending. Preserve distinct apps/runs with equal names.

Discriminating regression: keep the PR head fixed; return an old completed-success run
whose suite is queued after a rerequest. Assert pending and no terminal CLI success until
the suite/current attempt has completed. Also retain an ordinary completed-success case.
The current added fixtures contain neither suite state nor a rerun transition.

Evidence: source inspection and the [GitHub rerequest contract](https://docs.github.com/en/rest/checks/runs#rerequest-a-check-run).
This provider scenario was not exercised through the local API/DB owner in this review.

### F2 · P2 · Test-merge checks are silently omitted from a PR observation

Affected path: `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:236–252`
and `:275–303`.

`head()` discards the rest of the PR identity and enumeration queries only `head.sha`.
GitHub permits a PR's displayed/evaluated status checks to belong to its test merge commit;
the live PR head and test merge revision are different facts. With a passing head check
and a failed status on the current test merge commit, this code returns `ready` containing
only the passing head check and the CLI exits 0. With checks only on the test merge commit,
watch can wait forever on an empty head set although the PR already has a failure.

The user requested the PR's check state and GitHub-published failure evidence, and the
design explicitly left synthetic-merge revision behavior to source/protocol verification.
There is no supported-case exclusion or incomplete-data warning here. This is a missing
PR observation, independently of merge authorization or a required-check inventory.
Resolve the selected PR's relevant test-merge observation alongside its live head, retain
the actual revision on rows, and keep `--head` bound to publication identity. If this
version cannot inspect that case, explicitly report unavailable/incomplete instead of
a complete passing PR observation.

Discriminating regression: fixed PR head A and current test merge M, with head check A
passing and a legacy status/check on M failing; assert the failure is visible with M's
revision or the overall observation explicitly refuses completeness. Repeat with only
M checks. The current tests only supply a `head.sha` and never a test merge revision.

Evidence: source inspection and [GitHub's head/test-merge status selection](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks#conflicts-between-head-commit-and-test-merge-commit).
No live GitHub or local provider fixture for this case was run.

### F3 · P2 · Annotation failure discards diagnostics already retrieved successfully

Affected path: `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:417–429`
and `:344–349`.

The check-run enumeration has already supplied `output.summary` and `output.text`, but
`diagnostics()` attempts annotations with `await?` before constructing details. A single
annotation request failure (including an oversized annotation body or transient 5xx)
replaces all details with one unavailable message. The available compiler/test summary
and text disappear, leaving the agent unable to use evidence GitHub did return.

This conflicts with `g5.governance.pr.checks.details`: expose available GitHub-published
failure evidence and explicitly mark the missing/bounded portion. Preserve and redact
the retrieved summary/text, mark annotations unavailable or partial with the error, and
retain truncation/availability evidence instead of replacing the whole diagnostic object.

Discriminating regression: a failed run has a useful summary/text and positive annotation
count; the annotations endpoint returns 500 or a body above 2 MiB. Assert the original
redacted summary/text remain in API and CLI details and only annotation completeness is
unavailable. The added tests exercise annotation success and truncation, not failure.

Evidence: exact control flow inspected; this API failure path was not executed locally.

### F4 · P2 · Diagnostic rate limits do not stop watch or subsequent provider requests

Affected path: `backend/g5-api-core/src/services/workspace_pull_request_checks.rs:344–351`,
`:374–375`; `backend/g5d-cli/src/pr.rs:142–160` and `:546–582`.

A 429/403 with `Retry-After` from annotations becomes only `check.details.availability =
unavailable`; the repository remains `ready`. The server continues to other checks and
the final head request. If another check is pending, CLI watch ignores the diagnostic
error and polls again after its normal interval, even when the provider explicitly said
to wait longer. A provider-rate-limited read of the requested details thus repeatedly
hits the same limit instead of stopping as the design/docs promise.

Preserve structured retrieval/rate-limit information through the provider operation and
make the requested-detail error stop watch (or honor a structured retry deadline). Stop
additional provider requests in the current observation after the rate limit. Retaining
already-fetched summaries from F3 is compatible with this failure result.

Discriminating regression: one failed run with annotations returning 429 and
`Retry-After: 60`, plus one pending run. Assert no further immediate provider requests and
that `--watch --details --interval 1` does not issue another API read after one second.
Cover primary 403 rate limits as well as 429.

Runtime evidence at the actual CLI consumer: an isolated loopback API returned exactly
the shape this backend emits (repository ready; failed check details unavailable with a
60-second retry message; another check pending). The compiled `target/debug/g5d` issued
two requests **1.004984 seconds apart** and exited 0 after the second fixture returned
pass. stdout contained only the final JSON; stderr displayed the rate-limit message.
This proves the CLI decision, not live GitHub or the complete backend path. The backend
error-to-shape and continue behavior is source evidence. The temporary server used dummy
credentials and an isolated temporary home; no shared DB/test resources were used.

## Commitment coverage and evidence ledger

| Commitment | Actual path and discriminating case | Assessment |
|---|---|---|
| `g5.governance.pr.checks` | New reader resolves G5/current/provider selectors, retains all provider rows; CLI evaluates every repository before JSON projection. Missing-provider detection reuses `has_missing_provider_pull_request`. | Source supports multi-repository partial errors, pending empty results, explicit limits, and no arbitrary member selection. F1/F2 affect completeness/currentness. Existing fixtures cover default/G5/multi-repository/partial failures, but this reviewer did not run the API suite. |
| `g5.governance.pr.checks.revision` | Initial and final live head reads, expected-head comparison, run `head_sha` validation, CLI repeats operation. | Old-head mismatch and changed-head enumeration are handled and have discriminating API fixtures. Same-SHA rerequest is F1; test-merge distinction is F2. Multi-page list mutation and multi-repository terminal revalidation remain unexecuted. |
| `g5.governance.pr.checks.details` | `diagnostics`, bounded annotation request, output redaction/truncation, status description/link, safe links. | Check/status diagnostics and link-only limitations are implemented. F3 loses available evidence; F4 loses retrieval control information. No external diagnostic URL is fetched. |
| `g5.governance.pr.read`, `.json` | Extracted `attached_repository` is equivalent to its former inlined policy; selectors use existing `selection`; `pr_json::Selection` validates fields and forces details demand. | Repository selection cannot expand attachment scope. JSON is an array, projection does not change exit status, bare fields discovery precedes context/network. CLI tests cover core exits/watch. Invalid repository/URL combinations are deferred to API rather than all rejected locally as the plan says; this is a minor validation mismatch, not an authority bypass. |
| `g5.security.read`, `.tenant`, `.tenant.rls` | Public `get_pull_request_info` obtains `scoped_db_for_workspace(... workspaces:read)`; G5 query and attached-repo lookup use scoped DB; credential resolution revalidates membership and repository URL. | Preserved by inspection. New tests include outsider and other-repository refusal. Positive invited/nonowner reader and stopped durable-read checks for this operation are not exercised by the new fixtures. |
| `g5.security.admission` | Internal `get_pull_request_info_internal` obtains `scoped_db_for_workspace_incarnation` and `require_live_workspace_incarnation` each poll. | Existing typed path reused without mutation-only admission; stale-epoch refusal has a new fixture. Actual deployed VM and authority changes during a provider request remain unexecuted. |
| `g5.security.credentials.publication` | `ReadPullRequestChecks` maps to the existing repository credential owner and narrow installation permissions for pull requests/checks/statuses. Tokens only enter server requests and are redacted from observations. | Installation permission admission and mint payload have protocol fixtures. No new fallback is introduced. Personal-SCM checks and live installed App grants remain unexecuted. |
| `g5.security.credentials.redaction` | Effective workspace/owner config secrets are loaded before releasing the scoped DB; token/config values are redacted from final JSON; diagnostics are redacted before truncation. | Exact configured-value redaction has a fixture. Configured secrets at another member's source context and historical/overridden secrets were not evaluated. No claim of comprehensive secret detection. |
| `g5.governance.pr.write`, `.follow-push`, `.publish.readiness` | Checks route branches into a separate read operation; reused list/missing-provider/credential helpers were inspected for effects. | No save/push/synchronization/compute-start/CI-write calls or schema changes added. Existing publication code changes only expose the read function. Token minting remains the necessary provider-auth effect. |
| `g5.governance.approval`, `.approval.version`, `.merge` | No approval or merge path is called; readable output says observed checks are not merge approval. | Preserved by inspection; no new authority is inferred from success. F1/F2 remain check-observation defects even under this disclaimer. |
| `g5.data.git-authority`, `.platform-authority.vm-access`, `.outcome-evidence` | CLI calls authorized API; checks derive from provider reads rather than stored G5 SHA/status or successful push. | Correct authority boundary and no new durable cache. Current-attempt/test-merge omissions limit the derived outcome (F1/F2). |
| `g5.architecture.pr_checks` | Public/internal adapters share one read owner; provider protocol stays server-side; CLI owns cancellable polling. | Minimal useful extension without a queue/monitor/table. Provider rate-limit handling is incomplete (F4). |
| `g5.design.reuse`, `.minimal-mechanism`, `.adapters`, `.authority`, `g5.architecture.policy` | Existing route, scoped auth, config and repository credential facilities reused; read/write policies remain separate. CLI process shell preserves checks exit codes. | No unjustified new service or persistence mechanism found. The two nested 120-second timeouts are redundant but not an actionable functional defect. |
| `g5.quality.review`, `.review.cases`, `.review.producer`, `.evidence`, `.evidence.fakes`, `.evidence.unknown` | Full delta and affected route/credential/config/publication/CLI/harness consumers inspected; API provider fakes sit outside the actual owner. | Main agent owns shared suite execution; this review records inspection separately from the one compiled-consumer experiment. Repairs should be made at the shared provider read owner and its CLI result consumer. |

## Verification and remaining evidence

- `git diff --check` passed for the production/test/docs delta inspected.
- Compiled `g5d pr checks --help` succeeded after setting
  `DYLD_LIBRARY_PATH=/opt/homebrew/lib`; direct invocation initially failed to locate
  `libkrun.1.dylib`. This was a local runtime setup issue, not a code finding.
- The loopback consumer experiment initially could not bind under the filesystem/network
  sandbox, then ran successfully with approval for loopback-only execution. It did not use
  external network, real credentials, shared DBs, or a checkout mutation.
- API and CLI added tests were read in full. The main agent is executing the shared suites;
  this reviewer did not run them in parallel or claim their results.
- The `AGENTS.md` CI guidance reaches the ordinary worker harness through
  `scripts/stage-local-worker-runtime.sh:310–329`, which stages the repository-root file.
  Existing `.dev` staged assets are build outputs and are not changed by this task.
- Other unexecuted cases include actual provider grant installation, deployed VM execution,
  successful multi-page data aggregation, duplicate-name independent apps, new same-SHA
  check creation during pagination, current run/suite changes during reading, annotation
  API failure, cancelled watch signal behavior, and API-level deadline injection. These
  are evidence gaps, not asserted runtime passes.

## Ontology and design follow-up proposed to the main agent

The four new footer entries still say unimplemented. After coherent repair and shared
verification, update them to the actual reader/CLI paths, preserved limitations and exact
evidence, without presenting provider fakes as live GitHub or deployed VM proof. Update
the saved amendment's implementation assumptions with the resolved rerun and test-merge
protocol decisions; preserve the original requirement. No new architecture or durable
monitor is needed. Existing broad security/principle metadata should retain their wider
unassessed scope while linking this scoped evidence if their assessment changes.
