# Target/title/node-ID follow-up — independent review one

Independent read-only discovery against the frozen working tree represented by
`/tmp/g5-pr-followup-snapshot.tgz`. I did not implement this change, read the author's
review explanation, read another reviewer's report, or delegate. Inputs were the saved
request, the pinned `ontology-plan.md` (including its first section, recorded diff and
final target/title/node-ID amendment), current governing ontology bodies and metadata,
the review skill and design guide, implementation, tests, and permitted check logs.
No supplied defect hints were used. Only this report and isolated `/tmp` investigative
files were written.

## Initial findings

### F1 · P2 · Persist and fence pending provider title intent

**Defect, source-inspected.** `g5.governance.pr.title`, `g5.governance.pr.write`,
`g5.design.authority`, and `g5.architecture.recovery` require the explicit title to reach
G5/provider state and partial failures to remain honest and recoverable.

The local title is persisted at
`backend/g5-api-core/src/services/coordinated_repo_publication.rs:326` (and the
insert-race branch at 397), but the provider phase receives only the request's transient
`title` at `workspace_pull_request_create.rs:298`, after the local transaction commits
at 282. `PullRequestProviderContext` does not load the persisted title or any pending
title intent (`workspace_pull_request_provider_lifecycle.rs:113`, 1446–1457).
PATCH is conditioned solely on that transient option at 1048–1052, followed by an
unconditional `Open` status with `last_error: None` at 1055–1065.

Concrete failure/retry counterexample:

1. Existing provider PR is titled A. A request with title B commits G5 title B, but its
   PATCH fails. The response and stored provider row correctly report failure.
2. The user retries via the existing UI Update PR button (which sends no title), or
   `g5d pr update` without repeating the title.
3. Discovery finds the same open provider PR. No PATCH occurs, and its provider row is
   rewritten as open with the failure cleared. G5 remains B and GitHub remains A,
   while the operation now reports successful provider state.

The new test at `api_workspace_pull_requests.rs:1581–1608` ends immediately after
step 1, so it does not expose this regression. A failed initial provider creation has
the related problem: an omitted-title retry derives a new provider title rather than
recovering the previously persisted explicit title.

The same transient input also permits a stale write after overlapping requests:
A's local title A commits; B's local title B commits and its provider phase completes;
A's delayed provider phase then applies A even though the authoritative G5 title is B.
The fresh provider context uses current PR evidence but does not check its request title
against current persisted intent. Phase admission does not bind the title to the local
commit that accepted it. This interleaving is source-derived, not runtime-executed.

**Repair:** retain explicit/pending title intent in the PR owner's durable state and
have provider synchronization consume/fence that state, preserving already-synchronized
provider titles on an ordinary omitted-title call. Clear title-related partial failure
only after confirming or completing that title effect. Do not blindly force the local
auto-derived title onto all providers, because existing per-repository derivation and
omission semantics remain valid.

**Regression checks:** through the public/VM shared operation, fail a title PATCH,
retry through an omitted-title UI request, and verify the actual provider request/state
and persisted failure state. Also fail initial creation after a custom local title,
retry without a title, and assert the eventual provider title; use barriers between local
commit and provider admission for A/B requests and assert final G5/provider agreement.

### F2 · P2 · Refresh the new target query when workspace state changes

**Defect, independently reproduced using the actual hooks.** The shared target
projection implements `g5.governance.pr.target`, but its new cache family at
`frontend/src/hooks/useWorkspace.ts:435` is disconnected from workspace mutation and
reconnection invalidation. `useDeleteWorkspace` invalidates `workspace`/`workspaces`
at 260–264; visibility changes, clone and transfer similarly update those existing
families. `frontend/src/lib/ws.ts:751–755` and 1306–1328 do not invalidate the new
family either. The only explicit refresh is window focus.

`WorkspacePage.tsx:6298–6320` reads that cache for both Create PR and Prepare to Ship;
opening Create PR at 6375–6379 does not refetch. After a formerly eligible parent is
deleted or made inaccessible, the focused page can continue to display and explicitly
submit that old target while an omitted-target CLI call already selects the Base. A
newly eligible target can likewise remain absent, including a stale empty list that
keeps the UI action disabled. Server authorization still rejects invalid targets, so
this is a selection/recovery regression rather than an authorization bypass.

**Executed reproduction:** isolated Vitest test
`/tmp/g5-pr-review-one/target-cache.test.ts` mounts the real
`useWorkspacePullRequestTargets("source")` and `useDeleteWorkspace("parent")`, using
the repository's real MSW setup. After successful deletion changes the target endpoint's
answer to Base, it observes one total target GET, `isInvalidated === false`, and the
old parent still present. Result: **1 passed**, meaning the stale-cache counterexample
was reproduced. No production/test source was changed. The first attempt needed Vite's
filesystem allow-list adjusted to read the repository from the isolated test root; the
second attempt executed successfully.

**Repair:** connect the target projection to existing workspace invalidation/reconnection
events and refresh when opening the destination chooser. Keep backend selection policy
authoritative. Consider putting this query under an existing invalidated family rather
than maintaining another separate list of refresh call sites.

**Regression check:** mount the destination consumer, then delete/change visibility of
its parent (and separately create the first eligible Base); assert refreshed choices and
default selection without remounting or window focus. Preserve explicit user selection
when it remains eligible.

## Additional unverified boundary

The provider title PATCH is below `repo_needs_provider_pr` at
`workspace_pull_request_provider_lifecycle.rs:483`, which excludes identical/already-
included source histories at 2157–2164. A title-only update therefore never visits an
existing provider row when its current local comparison no longer needs a new PR.
There is no test establishing whether an existing open provider PR can persist in that
state in supported synchronization scenarios. Verify that valid existing-provider
variant; if supported, enumerate existing provider PRs for explicit title changes
independently of the gate for creating a new provider PR. This is recorded as an
evidence gap rather than a proven provider-state counterexample.

## Obligation and evidence ledger

| Commitment | Actual path and discriminating case | Result |
|---|---|---|
| `g5.governance.pr.target` | `workspace_pull_request_create::targets` checks source access, same-org live parent/Base candidates, excludes self/deleted rows, sorts parent/name/ID and filters with the real workspace-access owner. `create_or_update` resolves once before native preparation; later phases retain the selected UUID. | Backend preserved by inspection and existing HTTP checks; UI invalidation violation F2. Default Base ordering is directly tested at the projection, but a successful omitted-target write using the Base fallback is not runtime-proven by those assertions. |
| `g5.governance.pr.title` | Blank validation before target/save; shared request accepts title; local insert and both existing-PR branches retain explicit title; omission does not rewrite the local title. Provider POST and 422 legacy retry retain supplied title; existing PR uses PATCH. | Happy path covered by existing HTTP test/log; failure/recovery and accepted-title authority violation F1; unchanged-comparison provider boundary unverified above. |
| `g5.governance.pr.write`, `g5.architecture.pr_flow` | CLI create/update alias sends one POST to the same owner used by UI; adapters do not assemble preparation/publication. Both profile skills explain source, inferred/explicit target, title, save/push and partial provider outcomes. | Preserved in inspected follow-up. Both bundled PR skill files are byte-identical. |
| `g5.governance.publish.readiness` | Shared owner rejects invalid blank/no-default inputs before `prepare_pull_request_source`; successful flow saves/pushes, captures evidence, reuses G5 PR, publishes, commits, then synchronizes provider. | Preserved in inspection; current HTTP suite covers scripted native callbacks and failure/retry. Real active guest/native Git remains unexecuted. |
| `g5.security.owner`, `g5.security.admission`, `g5.security.login.service-identities`, `g5.runtime.lease` | VM `source_db` requires same source and current owner/VM/epoch; `lock_owner_handle` checks lease, running state, Base and transfer exclusion. `authorized_db` rechecks source mutation/review policy and target readability for each phase. UI uses actual authenticated user admission. | No new authority bypass found. Target access is not broadened by default resolution; invalid explicit targets still fail. Existing HTTP tests include replaced/stale/foreign/transfer/target-access refusals. |
| `g5.security.read`, `g5.security.tenant`, `g5.security.credentials.publication` | Targets call the existing actor access owner and remain inside source organization; GitHub lookup selects only attached repositories and resolves server-side read credentials; node-ID mapping adds no new request or credential scope. | Preserved by inspection and existing read HTTP tests. |
| `g5.governance.publish.github-author`, `g5.governance.publish.actor` | New provider PR still uses `CreatePullRequest` actor credentials; reused PR title PATCH uses the existing `WritePullRequests` operation policy. Source publication retains `WriteGit` policy. | No follow-up authorship regression found. Existing mock assertions distinguish actor creation from installation update; live GitHub remains unexecuted. |
| `g5.governance.pr.json`, `g5.governance.pr.read` | REST `node_id` becomes API `id` at `workspace_pull_request_read.rs:150`; common JSON selector includes `id`; G5 still projects its UUID. Invalid/bare fields return before context lookup or mutation. | Numeric and URL selector API and compiled-CLI tests assert `PR_node42`, distinguish it from numeric database ID and PR number, and preserve read-only paths. No defect found. |
| `g5.design.minimal-mechanism`, `g5.design.change-locality`, `g5.design.adapters`, `g5.design.authority`, `g5.architecture.recovery` | One narrow shared target projection; title follows existing orchestration/provider owner; node ID is local projection. Consumers and recovery investigated beyond the immediate diff. | No unjustified new mechanism found. F1 requires preserving one accepted title authority across phases; F2 requires maintaining the new projection. |

## Checks and limits

- Independently executed the isolated actual-hook cache reproduction above: 1/1 passed.
- Inspected `/tmp/g5-pr-followup-api-recheck.log`: current workspace PR suite 32/32
  passed. Earlier `/tmp/g5-pr-followup-api.log` had 4/4 read tests pass and workspace
  failures; the later complete 32-test run supersedes its workspace result.
- Inspected `/tmp/g5-pr-followup-cli.log`: compiled PR suite 12/12 passed, including
  inferred source/target/title transport and numeric/URL node-ID output.
- Inspected `/tmp/g5-pr-followup-ui.log`: 4 files, 69 tests passed. Target test checks
  initial backend order only; it does not exercise projection refresh.
- `/tmp/g5-pr-followup-units.log` reports 75 passed and 2 host tests unable to bind a
  mock server under the sandbox. Those failures are outside this follow-up; the scoped
  PR unit tests in that log passed. No claim of a clean complete unit run is made.
- No broad Cargo build or shared database test resource was used by this reviewer.
  Provider recovery/concurrency counterexamples are source evidence, not executed
  HTTP regressions. No live GitHub or deployed worker was exercised.

Initial discovery is complete; implementation is not fully verified or defect-free.
Main-agent consolidation should preserve the two initial findings, resolve the extra
provider boundary, apply one coherent repair, and then update the target/title/JSON/flow
metadata with current evidence. No ontology meaning change is needed to fix F2. Any
consequential persistence choice for F1 should be reflected in the existing design before
dependent implementation, without weakening title or honest-partial-failure obligations.

## Incremental repair review

Reviewed the durable-title design/ontology amendment, migration 041 and checksum entry,
atomic local-title/pending-title writes, provider pending-intent consumption and clearing,
recorded-member title synchronization, known-identity preservation, cache-key repair and
new regression. No production or test files were edited in this pass.

- **F2 closed by source and runtime evidence:** target queries now use
  `["workspaces", "pull-request-targets", workspaceId]`, so the existing delete,
  visibility, transfer and reconnect invalidations apply. The new actual-hook deletion
  test observes the replacement Base and two GETs. `/tmp/g5-pr-followup-ui-repaired.log`
  records 70 passing tests across four targeted files.
- **F1 sequential failure/retry repaired:** explicit local titles and
  `pending_provider_title` are written atomically. The provider owner consumes pending
  state; failed PATCH retains the pending title and provider identity; a title-less retry
  updates the mock provider's title and clears pending only on successful state. The
  new real HTTP test verifies failed creation, failed PATCH, title-less recovery,
  supersession followed by a later continuation, and preservation of a completed
  externally edited provider title. `/tmp/g5-pr-followup-title-repair.log` records that
  regression passing. The additive nullable migration preserves legacy omitted-title
  behavior, and the checksum entry matches the new migration path.
- **Original included-member evidence gap closed:** the new regression changes actual
  member heads to equality, keeps a sibling outgoing, and verifies the existing provider
  title changes without another POST. `sync_recorded_title` validates repository identity
  and uses the existing server-side write credential owner.

### F1 concurrency remainder · P2 · User provider phases still lack a title lock

**Defect remains, source-inspected; exact concurrent HTTP schedule not executed.**
Removing the stale request argument fixes continuations that load context *after* a newer
request completes. It does not protect an already-loaded pending title while provider I/O
is in flight. The provider owner loads context without a row lock at
`workspace_pull_request_provider_lifecycle.rs:472` / `load_context` (1566 onward).
The caller's user path only acquires `FOR SHARE OF workspace` through
`workspace_mutation_policy.rs:279–286`; unlike the VM path, it does not acquire
`lock_owner_handle`'s exclusive workspace lock. PR EvidenceCapture revalidation uses
plain workspace/snapshot reads (`job_merge_service.rs:650–662`, 914–939, 1882–1900),
and local title UPDATE locks only the PR row. No PR-row lock is held across the user
provider phase's context read and PATCH.

Allowed interleaving with already-published source branches:

1. User request A commits title/pending A; its provider phase loads A, then waits before
   the provider completes PATCH.
2. User request B commits title/pending B while A holds only a compatible workspace
   SHARE lock. B's provider phase patches B and clears pending B.
3. A's provider PATCH completes later with A, records open status, and its conditional
   clear at 572–575 affects zero rows because pending is already null.

Final state is local title B, provider title A, no pending intent and no error. The
conditional clear avoids deleting B's pending string while present, but cannot fence
the earlier external effect after B successfully cleared it. The new regression at
2963–2980 is deliberately sequential and does not discriminate this schedule.

**Remaining repair:** serialize provider title consumption/effects with accepted local
title updates for both human and VM callers, using a consistent lock/order that spans
context read through provider transaction commit (for example, lock the authoritative
PR row before reading pending intent, and ensure local updates follow the same order).
Avoid relying on VM-only source admission. Confirm this also serializes two human
provider phases and does not introduce a lock inversion with workspace/PR operations.

**Required check:** two signed-in-user requests, with a mock PATCH barrier pausing A
after A has loaded pending intent. Let B attempt its local/provider phases, release A,
and assert final G5/provider title agreement with no lost pending obligation. This must
exercise the public HTTP path; the VM path's stronger lock would hide this remainder.

No other new repair defect was identified in the inspected target, JSON, migration or
included-member paths. F1 should remain open until this concurrency remainder is
addressed; full API regression is being run separately by the main agent.

## Final bounded concurrency recheck

**F1 concurrency remainder (consolidated as F3) closed.** The provider owner now takes
`SELECT ... FROM workspace_pull_requests ... FOR UPDATE` before `load_context`
(`workspace_pull_request_provider_lifecycle.rs:472–479`). The caller retains that
transaction through provider I/O, status recording, pending-title clearing and commit
(`workspace_pull_request_create.rs:286–306`). The local title UPDATE conflicts with
this same row lock, so neither a newer accepted title nor a second provider phase can
overtake a provider operation that already consumed pending intent.

The inspected create/update paths retain workspace-admission-before-PR lock order for
both human and VM callers. The provider body does not acquire a later workspace write
lock or call native save/push; its source publication helper uses Git I/O. No new lock
inversion was found in this bounded repair inspection. This is not a global proof for
all existing merge/refresh/close interleavings.

The amended real HTTP regression uses two public, signed-in-user requests (not VM
tokens). A's PATCH response is delayed for five seconds. While A's provider transaction
is still active, it observes B waiting on its actual local title UPDATE via
`pg_stat_activity`, and fails if B completes instead of waiting. After both requests
complete, it verifies local result title B, mock-provider title B and persisted pending
title null. The lock-wait assertion would fail without the new PR-row fence, even though
the mock applies its effect before delaying the response. This therefore discriminates
the relevant serialization guarantee instead of relying only on final state or a sleep.

Inspected `/tmp/g5-pr-followup-concurrency.log`: the focused HTTP regression passed
(1/1, 7.01 seconds). This recheck inspected source, test and its recorded execution;
it did not rerun the shared database suite. The main agent is separately running the
full API suite. All findings from this reviewer are now resolved on the inspected
paths; previously documented live GitHub/deployed-worker limits remain unchanged.
