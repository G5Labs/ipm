# Issue #403 implementation review

Self-review against the saved [design](ontology-plan.md) and [request](../../chat_intent/remove-redundant-api-restarts/request.md). No independent reviewer was used.

## Initial findings

No implementation defect found in the scoped diff. The pipeline still publishes/imports the worker image and preserves the independent, disabled-in-QA host rollout. The deploy gate remains before Helm and uses the worker trigger's full-range input filters. The obsolete trigger settings are removed. The initial implementation also removed the dedicated bake cluster grant; the later review finding below narrows that cleanup. The reset script deliberately keeps its false re-pin override because it can build an older commit.

The shared API error fix covers JSON and raw-response modes while preserving server-provided errors. Ticket acquisition now uses the singleton API client, so refresh coalescing and principal replay checks apply. The socket boundary also rejects an empty or whitespace ticket. Destination recovery observes actual successful detail reads, including structurally unchanged data, skips manual cache writes and unrelated workspaces, and never cancels an in-flight target request. It needs no additional timer or durable state.

## Governing inventory and verification

| Commitment | Actual path | Counterexample/check | Result / gap |
|---|---|---|---|
| cloudbuild_pipelines.D35: gated deploy owns API pin | Bake YAML, QA delivery/foundation Terraform, deploy resolution and Helm dependency | No API mutation in bake; import/publish retained; gate configured before Helm. Existing full-range gate tests include intervening, queued/running, missing/failed and disabled bakes. | 98 Python tests pass across gate, resolver, pipeline ordering and reset. Both YAML files parse and Terraform 1.9.8 formatting passes. New contract fails against the original pipeline snapshot. Live QA rollout count unobserved. |
| g5.workspace.page.refresh: useful reason, same transient policy | api.ts JSON/raw error paths | Empty-status-text non-JSON 502 and structured 403 in all three request modes | Regression failed with blank text before the fix; status and transient classification preserved. |
| g5.workspace.page.refresh: retain drafts and recover destinations | useWorkspacePullRequestTargets and mounted WorkspacePage PR dialogs | Prepare to Ship draft / Create PR selection survive failed reads, then recover from only a successful detail read; original manual retry and target removal tests retained | Both automatic cases fail without the fix and pass with it. |
| g5.workspace.page.refresh: scoped recovery, no replay loop | Query-cache success subscription | Repeated failed retry; unchanged successful detail; late target failure after recovery; in-flight read not canceled; unrelated workspace/manual cache write ignored; initial failure and 403/404 not auto-retried | Hook suite passes; recovery test failed before fix. Empty loaded list also recovers. |
| g5.security.login.principal: reuse shared authentication | App ticket callback → api.post → refresh and replay guards | App+real socket manager gets 401, refreshes and retries with renewed token; API suite retains cross-principal refusal cases | Passes; old App opens an empty-token socket instead. |
| g5.workspace.page.refresh: keep session through temporary outage | Ticket callback and WebSocketManager backoff | 502 and empty ticket produce no handshake; timed retry later opens a valid-ticket socket | Both fail before fix and pass after it; existing reconnect/cache reconciliation suites pass. |
| g5.design.minimal-mechanism | Existing gate, query cache, API client and backoff | Check for new timers/state, alternate pin writers and in-flight cancellation | None introduced; one subscription follows successful detail reads. |
| g5.quality.delivery / authorization | Local checks and workspace PR | Candidate validation without applying infrastructure or restarting QA | Current local evidence below; live rollout is post-merge work, no deployment performed. |

## Later review finding: automatic Terraform apply

P2: removing `images_gke_repin` schedules a resource deletion in the foundation root. `cloudbuild-terraform-qa.yaml` refuses every delete/replace by default, before reaching delivery, so that cleanup would block the normal automatic apply. The original high-level design included grant removal; amend it to retain the unused grant with an accurate comment and leave its revocation to explicit infrastructure cleanup. D35's API ownership contract does not depend on revocation. Fixed by retaining the resource and updating its comment; Terraform formatting passes. The original handoff is preserved in commit `330313f5`.

## Executed evidence

- Initial red run: 8 targeted failures (3 API modes, 2 mounted automatic-recovery cases, 3 App/socket cases); 28 existing cases passed. Hook recovery separately failed before implementation. Pipeline ownership contract fails against the initial commit's pipeline snapshot.
- Final focused Vitest run: 13 files / 139 tests pass, including the entire WorkspacePageBasePull suite, App, API, workspace hooks and all WebSocket suites. The separate auth-context suite adds 34 passing tests (173 total).
- Python: 98 tests pass (`test_await_worker_image`, `test_resolve_worker_image`, `test_cloudbuild_waitfor_order`, `test_reset_qa`).
- ESLint for changed TS/TSX: zero errors, nine pre-existing warnings. Formatting and diff whitespace checked.
- TypeScript production check passed. Vite bundling aborts around 1.7 GiB with `NewSpace::EnsureCurrentCapacity` / allocation errors in this VM (8 GiB process data limit), including attempts with larger heap and smaller young-generation settings. No successful production bundle is claimed.
- Local browser-stack preflight stops before startup: `check_local_validation_contracts.py` flags four existing string assertions at `tests/e2e/src/scenarios/ci_e2e_lane_isolation.rs:651,668,670,768` as fixed Compose identities. They are assertions about CI lane wiring. No test-stack containers or images were present.

- Trigger rollout compatibility: Cloud Build trigger invocations force `ALLOW_LOOSE`, so an existing trigger still passing the retired substitutions does not reject the new bake config. The reset script invokes the trigger and can retain its false override for old commits. [Google Cloud documentation](https://docs.cloud.google.com/build/docs/configuring-builds/substitute-variable-values#using_user-defined_substitutions).

## Bounded reverse pass

Checked each changed governing condition against the inventory: standalone bakes wait for a later deploy; a failed gate performs no rollout; server refusals still replace stale content; warnings clear only after their own successful destination read; drafts/selections survive; unchanged data and late responses still recover; retries do not cancel requests or repeat across principals. No new contract amendment or additional defect was found. Unchanged deployment authorization and existing initial-load/refusal semantics remain in scope.

| Governing ontology | Remaining gap | Proposed action | Uncertainty / blocker |
|---|---|---|---|
| g5.quality.delivery | Production bundle unexecuted | Run the normal build in CI | VM allocation failure persists after heap tuning; TypeScript passes. |
| g5.quality.delivery | Full local browser stack unexecuted | Use current-head Cloud Build CI for stack coverage; repair the separate preflight false positives independently | Existing fixture identity scanner blocks local startup. |
| cloudbuild_pipelines.D35 | Actual QA rollout count | Observe the first worker-touching merge after this change deploys | No live deployment was authorized or performed. |
