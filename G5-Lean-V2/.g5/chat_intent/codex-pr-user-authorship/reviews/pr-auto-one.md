# Automatic PR push refresh — independent review one

Read-only discovery, 2026-09-30. Reviewed the frozen checkout and supplied
`/tmp/g5-pr-auto-review-snapshot.tgz` scope. No production, ontology or test files
were edited. This report was saved before sharing findings or reading the author's
implementation account or peer reviews. The approved design, request and current
ontology were inputs; no supplied defect hints were used. This review did not
delegate further.

## Initial findings

### F1 — P1: Local preflight refresh can erase the automatic provider obligation

Location: `backend/g5-api-core/src/services/workspace_pull_request_create.rs:412-417`.

The scanner treats `source_heads == current published heads` plus
`pending_push_refresh == false` as proof that all push continuation work is done.
However `source_heads` is also written by the independent local-preflight Refresh
endpoint. That endpoint calls `apply_prepared_workspace_pull_request_refresh`,
commits the new evidence, and never runs provider synchronization or sets pending
push work (`backend/g5-api-routes/src/routes/workspaces.rs:5365-5399`). The same
evidence writer is also used by blocked merge attempts
(`coordinated_repo_publication.rs:578-627`).

Concrete sequence:

1. An existing PR and its provider row are at A, with pending false.
2. Push B successfully; GitHub's existing PR follows its branch to B.
3. Before the background scan, use the incoming PR list's Refresh action. It
   captures B in G5 `source_heads`, but provider `source_head_sha` remains A.
4. Every later startup/periodic scan returns false at line 417. The refreshed
   provider row is never recorded, even across restarts.
5. Merge rejects with “Workspace source changed since PR publication; update the
   PR before merging” at `workspace_pull_request_provider_lifecycle.rs:731-741`.

Thus the exact manual Update PR requirement this change removes can persist
indefinitely. A manual create/update that writes the new evidence and then fails
before completing its provider phase can hide work for the same reason.

Governing: `g5.governance.pr.follow-push`, `g5.architecture.recovery.progress`, and
the single authoritative owner of completion. A local evidence snapshot is not
proof of provider continuation completion.

Proposed repair: make automatic completion derive from a dedicated completed
publication version or ensure every evidence writer that can consume a newly
pushed source atomically leaves a durable provider continuation obligation. Do
not merely remove the no-op optimization and poll every provider forever.

Regression: real provider fixture, push B, invoke the public local Refresh HTTP
endpoint before the worker, run `run_once` without an event, assert provider
`source_head_sha == B`, pending clears only after synchronization, and the public
merge works without Create/Update PR. Also cover the blocked-merge evidence writer
and an explicit update whose provider phase fails. Evidence here is source/path
inspection; this exact interleaving was not executed.

### F2 — P2: Early refresh failure remains invisible in the normal ready/open UI

Locations: `workspace_pull_request_create.rs:385-398` and
`coordinated_repo_publication.rs:1714-1725` (also the list SQL at 1752 onward).

An error in `ensure_automatic_refresh_allowed` (for example the existing GitHub
PR GET returns 503 or credentials cannot be resolved) rolls back the refresh
transaction. The failure handler writes only pending/error fields, preserving the
previous `local_merge_status = ready`, provider status `open`, and provider row
with no error. The summary projects the new error only as `blockingMessage`.

The incoming list displays `blockingMessage` only for a non-ready local status;
ready rows explicitly use the tooltip “Ready to merge”
(`frontend/src/components/workspace/PullRequestList.tsx:208-244`). Its provider
column consumes only provider-row status/error (`:247-293`). The Create PR result
also consumes those old local/provider statuses (`CreatePullRequestDialog.tsx:336-367`),
and Prepare to Ship suppresses the blocking reason when ready
(`PrepareToShipModal.tsx:350-366`). The new WebSocket invalidation successfully
refetches a record whose new failure the UI does not render. The user continues
to see ready/open until attempting a merge. CLI text does show blockingMessage,
so this is an affected UI consumer gap rather than missing persistence.

Governing: `g5.governance.pr.follow-push` requires visible failure and honest
publication state.

Proposed repair: expose and render pending automatic synchronization/error
independently of local mergeability (or at minimum render this persisted message
even when local status remains ready). Preserve the distinction between a
locally mergeable version and a completed provider refresh. This does not require
weakening merge-time checks.

Regression: use the early-503 case already present in
`automatic_pr_refresh_waits_for_remote_push_recovers_and_keeps_merge_checks`, read
the public summary, and mount the real incoming PR/Prepare to Ship consumer with
that ready/open response. Assert the pending/error is visible, then recovery plus
the update event removes it. Current UI evidence tests only invalidation and do
not cover this rendered outcome. Finding is based on source inspection.

### F3 — P2, admission-policy concern: successful pushes from read-only/freezing sources never refresh

Location: `workspace_pull_request_create.rs:118-124`, reused by
`refresh_existing_after_push` at lines 411 and 429.

Automatic continuation calls `ensure_workspace_mutable`, which classifies the
operation as `DirectWorkspaceWrite` (`workspace_mutation_policy.rs:246-259`).
That blocks `read_only` and `freezing` workspaces before checking whether there is
even new push work. Retained policy explicitly permits Git receive-pack,
publication and completion of accepted Git imports on these workspaces
(`workspace_mutation_policy.rs:34-46`, module documentation at :11-12).
Consequently an owner can successfully push an existing PR's source while the
new continuation retries `WORKSPACE_READ_ONLY` forever. The freezing case also
blocks a drain's already-admitted commit/push completion.

Governing: successful-push continuation in `g5.governance.pr.follow-push`, the
narrow committed-work variant in `g5.architecture.pr_flow`, and retained mutation
policy (`g5.scope.retired-mechanisms.mutation-policy` defers its retirement).
The conclusion that this continuation is accepted-Git completion rather than a
new content write is an interpretation of those contracts, explicitly identified
for consolidation. The refusal itself follows directly from the code.

Proposed repair: keep current source ownership/transfer fencing and target access,
but use operation-specific automatic publication admission rather than inheriting
the explicit button's content-mutation gate. Keep explicit save/title inputs under
their current policy. If this exclusion is intentional product policy, the
automatic-push commitment needs to state it instead of promising all successful
source pushes.

Regression: create an open PR while writable, transition source to read-only (and
separately freezing), perform a permitted native Git push, and verify the same PR
follows it without guest save/start. Also verify that explicit content writes
remain refused. This case was not executed.

## Scope and obligation ledger

| Commitment | Actual path and check/counterexample | Assessment |
|---|---|---|
| `g5.governance.pr.follow-push`: existing PR only, stable identity/target | Maintenance discovers only open PR IDs; refresh loads the existing source/target and never creates a G5 PR. Closed-state checks run after the PR lock. | Preserved by inspection and supplied closed/absent tests. |
| Published committed vector; no dirty snapshot, autosave or compute | `published_heads`, `PublishedEvidenceCapture`, `resolve_published_snapshot`, `synchronize_after_push`; inspected manual save/push and provider push branches to establish exclusion. | Preserved by inspection; native push and two-member dirty-retained-snapshot tests pass in supplied logs. |
| Successful push remains successful if PR refresh fails | Git accepted-ref event is a hint; background owner is independently spawned. Error persistence occurs after failed refresh. | Preserved by inspection and supplied API/native tests. |
| Durable discovery, retry, coalescing and no lost work | Startup interval plus branch hints; current published vector checked before capture and again before commit; pending survives ordinary provider action-required results. Other evidence writers were traced. | **Violated by F1**; tested provider-failure recovery otherwise passes. |
| Visible pending/provider failure | Error persisted, summary exposes blockingMessage; CLI text consumes it; actual frontend consumers inspected. | **Violated by F2** for ready/open rows. |
| Current owner, eligible organization scope, target visibility, no old pusher grant | Discovery uses scoped maintenance identity only to find current owner. Effect phases remint user scope, check source owner/transfer and current target access. Provider credentials remain in owner service. | Inspected; supplied target-access loss/recovery test passes. Read-only/freezing variant is F3. Ownership transfer and revocation during provider wait are unexecuted. |
| `g5.governance.approval.version` | Refresh updates PR evidence and events, never approval statuses. Supplied test compares reviewed_git_heads and reads non-approved state after the push. | Preserved on inspected/tested paths. |
| `g5.governance.merge` / `.effects` | Provider merge now takes the same PR row lock; automatic refresh checks recorded and observed terminal provider state; active merge execution checked in preparation and revalidation. Merge public route persists provider acceptance before import. Merge verifies recorded source SHA, live PR SHA, remote refs and passes SHA in merge request. | Inspected and supplied sequential stale-source/merge test passes. Concurrent merge and automatic refresh, ambiguous merge response, and process termination are unexecuted. |
| `g5.architecture.pr_flow`, minimality and legitimate variants | Manual and automatic orchestration share coordinated evidence and provider lifecycle owners; routes do not duplicate provider SQL. Maintenance is a small open-PR recovery adapter; only two durable pending/error columns added. | Mechanisms have concrete responsibilities. F1 exposes a missing complete-operation boundary with other evidence writers; F3 concerns an over-broad reused admission variant. |
| `g5.design.commit-notify` and frontend refresh | Events publish after commit to source and target channels; frontend invalidates the scoped PR query key. Existing branch event bus is reused. | Preserved by inspection and supplied WebSocket tests; notification does not repair F2. |
| Migration/startup | Migration 042 adds nullable error and non-null false-default pending; checksum and startup worker wiring inspected. | No finding. |

## Evidence and remaining boundaries

- Read: pinned design's first governance section and automatic amendment, original
  request, actual changed governance/architecture blocks and affected metadata,
  security admission/read/tenant rules and recovery/minimality rules.
- Retrieved the publication callers/consumers, public local Refresh and Merge
  routes, provider merge/import lifecycle, workspace snapshot readers, mutation
  policy, event bus, PR list/Create PR/Prepare to Ship UI, and CLI blocking-message
  presentation. Earlier title/authorship/CLI work was considered only where the
  automatic continuation consumes it.
- Supplied runtime logs: one native push test, three automatic API tests, and four
  WebSocket unit tests passed. Parent separately reported the full 36 PR + 4 read
  + 5 native API suite and typecheck passing; those are reported evidence, not a
  new execution by this reviewer. This reviewer ran scoped `git diff --check`
  successfully; no shared mutable test resources were used.
- Progress under a hung remote remains unverified: `run_once` awaits PRs serially,
  and the automatic source/target `GitRepo::ls_remote_ref` calls are synchronous
  libgit2 network calls without an explicit owner deadline. The service's 30-second
  reqwest timeout does not bound those calls. A stalled remote before a later
  eligible PR needs a bounded-progress test; add a bounded blocking-network
  mechanism if the transport cannot establish the recovery-progress obligation.
- External GitHub merge/closure between the automatic terminal-state GET and
  `find_open_pull_request` is untested. The latter can create a new provider PR
  when discovery returns none, so the recorded provider identity and completed
  merge must be checked under that race, separately from the G5 PR lock test.
- Existing-member transition to `Identical`/`SourceAlreadyMerged` is untested:
  synchronization skips that member unless title intent is pending, while provider
  merge still validates every recorded row's source SHA. Cover a formerly changed
  member becoming included while another member remains changed, and verify no
  stale provider pin prevents the promised merge.

No repairs or ontology metadata updates were applied. The main consolidation
should retain these initial findings, adjudicate F3's policy interpretation, repair
confirmed defects, and update follow-push/pr-flow metadata with consumer-level
evidence and any still-unexecuted boundaries.

## Incremental repair recheck

Read-only recheck of consolidated A1–A8 after the first coherent repair, preserving
all initial findings above. Scope was the changed continuation, coordinated
publication, provider lifecycle and actual UI consumers; no broad re-audit of the
earlier CLI/title/authorship work. The main agent requested a specific check of
manual versus automatic refresh after provider merge acceptance.

### R1 — P1: manual refresh/update can still replace an unresolved accepted merge

The automatic-only guard protects a provider merge awaiting local import, but
the shared evidence/provider owners do not enforce the same invariant for manual
entry points. `apply_prepared_workspace_pull_request_refresh` checks only G5
`status = open` before updating source/target evidence. The Create/Update reuse
path at `coordinated_repo_publication.rs:318-346` does the same. An accepted
provider merge deliberately leaves the G5 PR open until import finishes, so it
passes both paths.

Concrete sequence: provider PR #21 is accepted at A, local import fails, then the
source pushes B. Automatic continuation refuses to repin (correct). A public
local Refresh can nevertheless replace the G5 `source_heads` with B. A manual
Create/Update goes further: `synchronize_provider_pull_requests(after_push=false)`
does not call the terminal guard; without a title it does not even load recorded
provider rows (`workspace_pull_request_provider_lifecycle.rs:564-568`). Its
open-PR lookup finds no open #21 and can create #22, after which
`record_provider_status` replaces the existing number/status/source pins
(`:2210-2222`). The accepted #21/A evidence required for recovery is lost. Running
automatic refresh before/after manual refresh cannot restore that accepted
identity.

This is a remaining violation of the existing `g5.governance.merge` and
follow-push invariant, not a request to redesign merging. The new terminal-pin
test at `api_workspace_pull_requests.rs:3701-3724` currently exercises only the
automatic refusal and therefore misses this interleaving.

Repair: enforce terminal/accepted-merge preservation under the PR row lock at
the shared evidence/provider transition, including manual local Refresh,
Create/Update reuse and blocked evidence writes. A PR awaiting import must finish
recovery before its source pins or provider identity are replaced. Add a public
HTTP manual Refresh and Create/Update to the terminal-pin regression; assert
stored G5 and provider pins/number stay A/#21 and no provider POST occurs. Source
inspection establishes this finding; the interleaving was not executed here.

### R2 — P2: A7 still has a PR-first workspace-locking writer

Changing `lock_pr_for_path_workspace` to `FOR UPDATE OF pr` fixes the automatic
versus local-Refresh upgrade, but
`coordinated_repo_publication.rs:1418-1434`, `lock_pr_for_target_workspace`, still
locks `pr, sw, tw` together. It is used by
`persist_blocked_workspace_pull_request_merge` at `:580-627`, which opens a new
transaction without first taking the sorted workspace locks.

An automatic refresh can hold SHARE on source/target and be about to obtain the
PR row. A blocked manual merge can obtain the PR row in the joined lock query
and wait to upgrade a workspace; the automatic transaction then waits for its
PR row. These paths retain the opposing PR/workspace order that A7 was intended
to remove. Unlike a successful local merge, the blocked-evidence writer has not
already gone through the merge application's sorted workspace locking.

Repair: give this metadata-only blocked-evidence writer a PR-only lock, or
acquire any genuinely required workspace locks in the same order before the PR.
Avoid changing the successful merge application's fencing semantics. Add a
conflicting-preflight merge versus auto-refresh lock regression that observes
the real waits and proves both operations settle. This is a static lock-order
counterexample, not the diagnosed cause of the current overlap-test timeout.

### R3 — P2: A3 remains incomplete on the source's PR surfaces

The DTO and incoming `PullRequestList` now expose/render pending/error correctly.
However `CreatePullRequestDialog.tsx` and `PrepareToShipModal.tsx` contain no use
of `pendingPushRefresh` or `pushRefreshError` as of this recheck. The source's
Create PR result still renders ready/open, and Prepare to Ship still suppresses
its reason for a ready PR (`:350-366`). The user whose push triggers automatic
refresh can therefore still see successful-looking publication on these primary
source-side surfaces during an early provider failure.

Repair: render the separate pending/error notice on both source-side consumers,
without conflating local readiness with provider synchronization. Add a mounted
source consumer regression with the same ready/open/pending DTO used by the
incoming-list regression. The existing eight passing UI tests establish the
incoming list and query invalidation repair, not these two consumers.

### Recheck ledger and evidence

| Consolidated finding | Recheck result |
|---|---|
| A1 | Evidence writes now atomically mark pending when source or target evidence changes; new PRs start pending; discovery and completion independently compare existing provider source pins. This closes the inspected lost-work counterexample. |
| A2 | Automatic source/target observation uses the existing bounded reqwest/GitHub read path and ReadGit credentials; no automatic libgit2 observation remains in that path. Reported bounded-read/later-organization test passes in api5. |
| A3 | DTO and incoming-list rendering repaired; source consumer residual R3 remains. |
| A4 | Continuation uses current owner plus GitOperation policy. Unchanged discovery happens before owner admission. Native read-only/freezing pass reported in api3. No residual finding on this change. |
| A5 | Continuation obtains sorted workspace SHARE locks, then PR, then current organization membership and invitation read locks and rechecks target access. The invitation-after-PR-wait test passes in api5. No new finding on the inspected target-revocation boundary. |
| A6 | Automatic synchronization considers recorded included members and reads the same provider number, without creating an empty PR. Reported included-member test passes. |
| A7 | PR-only lock fixes the main refresh helper; blocked-merge writer residual R2 remains. |
| A8 | Canonical repository identity checked before automatic recorded-PR observation; automatic synchronization reads that recorded PR and refuses terminal state again before normal success. Identity test passes. |
| Merge pin preservation | Automatic terminal refusal is implemented; shared/manual residual R1 remains. A remote failure before the second terminal read also deserves coverage because failure status upserts can rewrite provider pins. |

`/tmp/g5-pr-auto-repair-api5.log` finished with 5/6 automatic tests passing; the
existing remote-push/recovery test timed out at line 3273 while joining the new
manual-refresh/observed-lock-wait test. I notified the main agent and did not
attribute the timeout to production deadlock without further evidence. The
test's `observe_wait` requires catching a specific query wait and can itself
remain pending if the wait is not observed. `/tmp/g5-pr-auto-repair-ui.log` has
eight passing tests; typecheck success is parent-reported. No tests were run
against shared mutable resources by this reviewer, and no code changes were made.

## Bounded scope verdict after the main agent's clarification

The main agent established that the overlap-test timeout was a connection-pool
cycle: automatic work held one tenant connection/PR lock, manual Refresh held
the second connection waiting for that PR, and credential resolution waited for
a third connection. This is distinct from R2's static row-lock-order concern.
The proposed repair reuses the admitted tenant `ScopedDb` within the credential
owner while preserving its checks; this reviewer did not implement it.

**R1 scope correction:** the reported ability of explicit manual Refresh/Update
to replace an accepted provider merge predates this automatic-push amendment.
On the parent's clarified bounded scope, it is existing debt, not a newly
introduced regression or a required expansion of this repair. The original
finding is preserved above for lineage, but its manual-only counterexample must
not be counted as an automatic-refresh defect. I found no way for a concurrent
G5 manual operation to bypass the automatic terminal guard while both obey the
same PR row lock. R2 and R3 remain scoped affected interactions and the main
agent is repairing them.

### R4 — P1: automatic failure branches can overwrite pins after external acceptance

There is a separate, automatic-only counterexample after the terminal guard;
it does not require the older manual Update behavior.

1. The open G5/provider PR records A. Source B is pushed into G5 while remote
   publication is still at A (the intentionally supported manual/queued remote
   push case).
2. Automatic continuation's `ensure_automatic_refresh_allowed` GET observes the
   provider PR open at A. It stages G5 evidence for B in its transaction.
3. The automatic GitHub ref read returns A, so `source_publication` becomes an
   error. During this observation, the PR is accepted externally at A.
4. `create_or_update_one` handles the remote mismatch by calling
   `record_provider_status` and returning **Ok**
   (`workspace_pull_request_provider_lifecycle.rs:1065-1082` in the inspected
   version), skipping its later terminal PR GET/check at `:1131-1177`.
5. The upsert replaces `source_head_sha` with candidate B even though the
   provider is at/has accepted A. Automatic continuation commits because the
   internal source still equals B. On the next retry the terminal guard sees
   merged and refuses refresh, leaving B as the recorded pin for merged PR A.

The same pin-changing Ok branches exist for pre-provider WritePullRequests
credential refusal, unavailable token, target-ref observation failure and
action-required PR-read errors. A held G5 row lock cannot serialize an external
GitHub merge. The later terminal check only protects paths that reach it.

This violates the automatic continuation's accepted-merge preservation rule.
It also has a direct recovery consequence: merge preflight requires the
recorded `source_head_sha` to equal the merged PR's head and will reject B versus
A instead of importing the accepted merge.

Proposed narrow repair: on an automatic continuation failure, preserve the
existing provider pins/identity and keep retry work pending; the already-present
automatic Err/rollback path is one option. Do not advance an existing provider
pin from an unconfirmed candidate on an action-required/failed observation.
Recheck failure helpers, not only the successful provider response path.

Regression: mock the initial PR GET as open A, make the source-ref GET signal
external acceptance and return A while G5 is B, then have subsequent PR GETs
return merged A. Run automatic continuation, assert both existing provider pin
and accepted-merge evidence stay A (with a visible pending error), and retry
without repinning. This source-based counterexample has not been executed by
this reviewer; it is confined to the new automatic path.

## Final bounded incremental assessment

Reviewed the final repair deltas in the shared credential owner, automatic
provider error paths, narrowed PR locking and source UI notices. No additional
material defect was found in this bounded pass. No implementation changes were
made by this reviewer.

- **R4 closed on the inspected/tested counterexample.** Automatic credential,
  missing-token, source-ref, target-ref, PR-read, creation, legacy-creation retry,
  title-update and source-blocked failures return errors instead of committing
  an unconfirmed provider pin. The final provider response also requires the
  candidate head and an open/unmerged PR, including after title synchronization.
  `automatic_pr_refresh_never_repins_an_unconfirmed_remote_cut` passed: its ref
  response marks external acceptance at A while the candidate is B, and both the
  failed attempt and later retry retain A in local and provider evidence with
  durable pending work. The successful push is still independent of this error.
- **Credential pool-cycle repair inspected.** `resolve_in_scope` checks that the
  supplied ScopedDb actor and organization match the requested credential actor
  and organization, then enters the same `resolve_target` policy as ordinary
  resolution. Current membership, exact expected URL, active latest App binding,
  repository coverage, requested capability, user authorization for new PRs and
  personal-token fallback policy remain in that common owner. The existing
  `lookup_repository_selection` → `mint_repository_token` sequence is preserved;
  normal resolution commits its own tenant scope before network I/O, while
  automatic publication reuses its already-held scope without committing it.
  Automatic initial PR observation also uses this path. The two-connection
  manual Refresh overlap test passed in `/tmp/g5-pr-auto-overlap2.log`, and the
  final suite's App-scope test verifies narrowed installation-token request
  permissions and successful persisted pins. Explicit wrong-actor/org rejection
  was inspected, not separately executed in this review.
- **R2 closed by source inspection.** The target PR lock helper now locks only
  the PR row. Actual local merge application retains its existing upstream
  workspace/finalization fences; metadata-only blocked writes no longer ask to
  upgrade workspace locks after taking the PR lock. The full merge suite passes.
- **R3 closed on actual consumers.** Both Create PR and Prepare to Ship render
  `pendingPushRefresh`/`pushRefreshError` independently of ready/open state. The
  incoming list retains its repaired notice. Source-dialog and list regressions
  are included in the passing 49-test UI suite.
- **R1 remains explicitly outside this automatic amendment.** Its manual-only
  unresolved-merge repinning behavior is preserved as pre-existing debt, not
  counted as a new automatic defect or an unfinished repair for this request.

The supplied `/tmp/g5-pr-auto-final-api.log` completed with 41 PR API, 5 native
Git push, 4 read API and 1 migration-upgrade test passing. The supplied
`/tmp/g5-pr-auto-final-ui.log` completed with 49 tests passing; it contains React
act warnings but no failing tests. Scoped `git diff --check` passed in this final
recheck. These are supplied suite receipts plus independent source inspection,
not fresh executions of shared resources by this reviewer. Live GitHub/deployed
guest execution and arbitrary external timing remain unclaimed.
