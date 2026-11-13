# Independent automatic PR refresh review — reviewer two

## Scope and method

Independent, full-scope, read-only discovery of the automatic-push amendment in `../ontology-plan.md` and `../request.md`. Reviewed the frozen checkout represented by `/tmp/g5-pr-auto-review-snapshot.tgz`. Started from **Ontology changes and governance**, read the current changed blocks and governing security, approval, merge, publication and recovery commitments plus their metadata. Applied `ontology-review` and the architecture/minimality guide. No production, ontology, or test source was edited and no subagents were used.

Earlier CLI, target/title, and authorship changes are context only. No author's implementation explanation or peer finding was provided. The parent supplied test-log locations and later baseline pass counts; these are verification evidence, not independent reruns.

## Initial findings (preserved before consolidation)

### F1 — P2: Local preflight refresh can permanently suppress provider continuation

**Primary location:** `backend/g5-api-core/src/services/workspace_pull_request_create.rs:412-417`.

The automatic worker treats `current published heads == workspace_pull_requests.source_heads` with no pending flag as proof that a push was handled. However, these heads are also written by operations that do not synchronize providers or set `pending_push_refresh`: the incoming-PR **Refresh local preflight** endpoint (`routes/workspaces.rs:5380-5396`), and blocked merge evidence (`coordinated_repo_publication.rs:600-608`, via `update_pr_evidence` at 1321-1351).

Concrete sequence: existing provider PR is recorded at A; push B successfully; before the periodic worker runs, the target owner clicks Refresh local preflight. That commits G5 source heads B while the provider row still pins A and pending remains false. Every later startup/periodic pass returns at line 417. Provider merge then refuses the mismatched source at `workspace_pull_request_provider_lifecycle.rs:741-752`, still requiring the manual Update PR that this amendment removes. A blocked merge whose new preflight captures B has the same lost-discovery effect.

**Governing:** `g5.governance.pr.follow-push`, `g5.architecture.recovery.progress`; intermediate evidence is not provider completion.

**Fix:** Keep provider synchronization progress distinct from mutable preflight evidence, or atomically mark continuation pending in every non-provider evidence writer when the source cut changes. Compare authoritative provider completion as part of the early-return condition. Preserve local-only no-op behavior and prevent retries from repinning accepted provider merges.

**Regression:** Real provider fixture at A; push B to internal and remote branches; call the public target PR refresh route before any automatic pass; call `run_once`; assert provider source evidence becomes B and merge succeeds without Update PR. Also cover blocked-merge evidence captured before the pass.

**Evidence:** Source-path proof; the current tests do not interleave this existing public action before discovery.

### F2 — P2: One stalled Git probe can prevent all later refresh work

**Primary location:** `backend/g5-api-core/src/services/workspace_pull_request_provider_lifecycle.rs:991-1001`; serial consumer `maintenance/workspace_pull_requests.rs:62-69`.

The new automatic branch calls synchronous libgit2 `GitRepo::ls_remote_ref` directly inside the async task while holding the PR transaction and source ownership lock. Its implementation (`g5-scm/src/repo.rs:890-920`) performs network connect/list without a caller deadline. The only recovery loop awaits each PR serially, so a stalled remote exchange can prevent later PRs and organizations from being revisited and can prevent the loop's cancellation select from being polled. GitHub HTTP client's 30-second timeout does not bound this separate libgit2 transport.

The existing advisory pinger already wraps this same primitive in `spawn_blocking` plus a timeout (`maintenance/remote_advisory_pinger.rs:265-285`). The new automatic continuation needs a similarly bounded operation; an outer Tokio timeout alone cannot interrupt a synchronous blocking call in its poll.

**Governing:** `g5.architecture.recovery.progress`, automatic amendment's “without starvation” requirement.

**Fix:** Move the probe off the async executor, bound its wait, release/drop the PR transaction on timeout, record recoverable pending state, and proceed to the next PR. Prefer a bounded/cancellable transport so repeated stalls do not accumulate unbounded blocking tasks.

**Regression:** Put a stalled TCP/Git endpoint on the first PR and a healthy pending PR later in the pass; assert the healthy PR refreshes within the configured budget, the stalled PR remains retryable, and cancellation returns promptly.

**Evidence:** Source inspection of both transport and serial scheduling; no stalled-endpoint runtime probe was run. Existing passing provider tests use responsive local remotes and do not exercise this boundary.

### F3 — P2: Outer refresh failures are invisible in the incoming PR UI

**Primary location:** `workspace_pull_request_create.rs:389-397` and its consumer `frontend/src/components/workspace/PullRequestList.tsx:208-244`.

For a PR previously marked ready/open, an early refresh failure (e.g. GitHub GET 503 in `ensure_automatic_refresh_allowed`, or lost target access) records only `pending_push_refresh` and `push_refresh_error`. The list SQL exposes that error as `blockingMessage`, and the new event invalidates the query, but the incoming-PR UI defines readiness only from `localMergeStatus`. It renders “ready” and the tooltip “Ready to merge” whenever that old status is ready, so `mergeBlockReason` never displays the error. Provider rows retain their previous open status/lastError, leaving no other visible failure indication.

The failure/retry API test creates exactly this state at its first injected provider failure, but never mounts the incoming-PR consumer. CLI text does print blockingMessage; the UI does not.

**Governing:** `g5.governance.pr.follow-push`: failure remains visible and must not be presented as successful provider publication.

**Fix:** Expose pending-refresh state distinctly or render the refresh warning independently of local merge readiness. Keep the difference between local mergeability and provider-refresh completion clear; do not rely only on a blocking tooltip that is skipped for ready rows.

**Regression:** Mounted incoming-PR list with ready/open old evidence and a non-null automatic-refresh error; drive the update event/refetch and assert the pending/error message is visible and the row does not claim current publication is ready.

**Evidence:** Backend write, SQL projection, real query invalidation and rendered consumer inspected. Current WS test validates cache invalidation, not visibility.

## Commitment coverage and evidence

| Commitment | Actual paths and discriminating case | Result / gap |
|---|---|---|
| Existing open PR follows successful source push; no implicit G5 PR creation, retarget, save or compute (`pr.follow-push`, `pr_flow`) | Native receive-pack branch hints → maintenance scan → `refresh_after_push` → shared refresh/provider owners; continuation has no runtime save/start call | Implemented on inspected path; native push test passes. Lost discovery via another evidence writer is F1. |
| Published committed vector, no dirty retained snapshot (`publish`, `pr.follow-push`) | `PublishedEvidenceCapture` → `resolve_published_snapshot` for both source and target; final source read compares complete repo vector | Source inspection and multi-member stopped/dirty-receipt test support this. Failed/rejected native push convergence is covered by supplied baseline suite; an obsolete hint reads current refs rather than event payload. |
| Current owner, org and target access (`security.owner/admission/tenant/read`) | Maintenance scope discovers only; continuation mints current-user capability, `ensure_workspace_mutable` checks/locks current owner and pending transfer, target-access helper runs before capture and application | Target access loss/recovery test passes. Owner-transfer/revocation during provider I/O was inspected but not independently executed. No stale actor is deliberately retained. |
| Do not bypass external auto/manual push policy | `synchronize_after_push` chooses observation rather than `ensure_provider_source_branch_pushed`; existing repository credential policy retained | Provider test proves internal push does not cause extra external push and external publication later allows completion. Observation scheduling has F2. |
| Durable retry and no regression (`recovery`, `recovery.progress`) | Open-PR inventory, initial interval tick, periodic/event paths, pending flag, PR lock, revalidation and final source-head check | Provider outage/retry, missed-event discovery and no-op tests pass. F1/F2 violate required progress. No restart-process or stalled-resource test was executed. |
| Approval version remains unchanged (`approval.version`) | Refresh updates PR evidence only; approval consumer separately compares reviewed Git heads | Multi-member test verifies stored reviewed heads unchanged and approvals no longer appear approved. No approval record writer added. |
| Accepted provider/local merge remains pinned (`merge`) | Provider merge and auto refresh share PR row lock; auto observation rejects recorded/live completed PR; PublishedEvidenceCapture rejects active merge runs; provider merge revalidates source/target and SHA | Existing provider acceptance/import and race tests pass in baseline. No dedicated concurrent automatic-refresh-versus-ambiguous-provider-acceptance test was present/executed; this remains a verification gap, not an additional asserted defect. |
| Terminal/absent PR unchanged | Discovery and apply both check open status; no insertion path | Closed/absent cases pass; successful provider merge followed by scan yields no work. |
| Failure visible, committed notification (`pr.follow-push`, commit-notify principle) | Error committed separately; event after commit to source and target; list SQL prioritizes refresh error; WS invalidates scoped PR queries | Backend/CLI projection supports visibility, but incoming UI hides ready-row warnings: F3. |
| Shared owner/minimality (`pr_flow`, `minimal-mechanism`, `change-locality`) | Narrow continuation, one published-evidence mode, two PR columns, one recovery adapter and event; provider policy stays in existing owner | Added mechanisms are justified by durable continuation. No unnecessary broad architecture change identified. |

## Available checks and remaining limits

Inspected supplied logs: `/tmp/g5-pr-auto-native.log` (1 native automatic-push test), `/tmp/g5-pr-auto-api-3.log` (3 new automatic API tests), `/tmp/g5-pr-auto-ui.log` (4 WS authority tests), and `/tmp/g5-pr-auto-full-api.log` (36 PR API, plus parent-reported 4 read API and 5 native push API tests passed). Parent additionally reported typecheck and real WebSocketManager/QueryClient checks passing. These are reused baseline results, not independently rerun tests.

No live GitHub/deployed guest verification, stopped remote transport, actual process restart, automatic/merge race, or the F1/F3 regressions was executed by this reviewer. Source analysis establishes the three counterexample paths; runtime confirmation and fixes belong to consolidation/repair.

## Proposed metadata/design disposition

After repairs, replace the changed `pr.follow-push` and `pr_flow` “Unassessed” entries with exact implemented paths, available test evidence and retained live-provider/concurrency limits. Preserve the saved automatic amendment; no change to governing intent is needed. Record F1's progress marker distinction and F2's bounded recovery obligation as implementation decisions in the existing design only if helpful. Do not weaken the visibility or progress contract to accommodate these defects.

