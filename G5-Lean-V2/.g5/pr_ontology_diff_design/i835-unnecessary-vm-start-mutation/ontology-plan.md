# Workspace lifecycle actions stay with their workspace

[Issue #835](https://github.com/G5Labs/G5-Lean/issues/835) supplies the reproduction and
acceptance criteria. The owner requested design followed by implementation and PR
qualification. The authorized branch is `i835-unnecessary-vm-start-mutation`.

The governing change is [workspace lifecycle UI state](../../ontology/features/ui.g5.md#g5.ui.workspace-lifecycle-state).
Existing [Start consent](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start),
[Stop](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.stop),
[transfer](../../ontology/features/workspaces.g5.md#g5.workspace.transfer.formal) and
[content policy](../../ontology/features/workspaces.g5.md#g5.workspace.content-policy)
retain their authority. No backend behavior changes.

## Ownership and choice

The existing QueryClient mutation cache owns browser request facts. Lifecycle hooks
derive each workspace/action's latest state from those facts; no separate registry,
navigation reset, durable store or new coordinator is needed. Pending mutations survive
observer detachment and cache collection until they settle. Settled results use the
cache's normal retention; reload recovers authoritative VM state by polling, not by
persisting browser requests.

Reuse the workspace phase mutation pattern, with a stable subscription to lifecycle
mutations and render-time workspace/action selection. Inspection of the installed
`useMutationState` shows that changing its filters alone does not immediately refresh
its snapshot, so filtering only by the current workspace in that hook is insufficient.
Each mutation carries its submitted workspace ID in variables for the API request and
settlement effects. Transfer also binds its recipient and idempotency key at submission.
The shared hook checks cached pending lifecycle requests synchronously to prevent
same-workspace duplicate submissions before React has rendered the busy state.

Clearing state on navigation loses A's pending request on A → B → A. Filtering only the
observer's latest variables loses A when B starts. A keyed control alone cannot repair
the reused page's hooks. The mutation cache preserves both requests and already supplies
subscription, completion and bounded settled-state retention.

The page keeps one busy/error derivation for ribbon, banner and mobile. Existing page
view identity guards suppress notifications and follow-up dialogs from a departed view;
the originating workspace's cached result/error remains available on return. The server
still owns permission, lifecycle identity, reservations, quota and host boot concurrency.
Transfer purges private-file caches for the submitted workspace; file-tree and
file-content keys both carry that workspace ID. The workspace listing is still
invalidated globally. This keeps completion effects on A from clearing B's file data.

## Validation and scope

Exercise real hooks with held HTTP responses and real controls across navigation:
independent Starts, A → B → A before polling changes, both completion orders, failure and
success isolation, originating cache invalidation, Stop and transfer siblings, duplicate
submission guards, and capacity-wait outcomes. Cover actual WorkspacePage consumers and
late notifications. Run affected frontend tests, type/build/lint checks and browser
coverage; the PR's existing Rust/E2E checks cover unchanged backend admission and capacity
regressions. Record local, CI and QA evidence separately. QA deployment verification
requires the candidate to be deployed and an authenticated browser; do not equate local
or PR CI evidence with a deployed QA result.

Self-review: the design satisfies each issue acceptance condition without weakening
same-workspace or backend admission. No unresolved product decision blocks the authorized
implementation. The only new shared mechanism is a lifecycle-specific projection and
submission wrapper over the existing mutation cache.

## Implementation evidence

The lifecycle hooks share a workspace/action projection and synchronous pending guard.
WorkspacePage uses its existing view identity for delayed notifications and Finish review
follow-up after Stop. Backend files and admission rules are unchanged.

The pre-fix real-controls regression failed with B's stopped Start disabled. With the fix,
14 lifecycle hook/control cases cover held responses, both completion orders, errors,
offline submission, origin invalidation, remount and same-workspace guards. Four full-page
cases exercise ribbon, mobile, banner and delayed notification isolation. Affected existing
hook/control and full WorkspacePage tests pass. Full frontend TypeScript checking, the
new browser test's TypeScript check, scoped ESLint (existing warnings, no errors) and the
production build pass; the build required an 8 GiB Node heap in this VM.
The [PR description](https://github.com/G5Labs/G5-Lean/pull/836) records these local checks;
they do not establish CI or deployment success.

`workspace-lifecycle-navigation.spec.ts` submits two real Starts through workspace tabs
and releases their HTTP responses in reverse order while holding VM reads at their real
stopped snapshots. It passed in PR E2E on `dd548cd0c8026a31e2bb00872b201b265ef5ad73`
(build `06c3ad33-5a4f-4fe7-b376-d90a2978820d`, 19.4 seconds). The full E2E
lane was still running when this evidence was recorded. The Stop notification test also
uses the API’s `stopped` result so its late-success assertion exercises the success path.
Deployed QA verification is
not yet established; this workspace has no authenticated QA browser or deployed candidate.

Rust CI on `8cb76ba3925e59996fcb07467911d0ff9db70ba3` failed its dependency
advisory gate for inherited `zstd-safe` and `zstd-sys` versions (RUSTSEC-2026-0403
and RUSTSEC-2026-0410). The owner explicitly excluded security dependency fixes
from this PR on 2026-10-26. Dependency manifests and the lockfile remain unchanged;
this is an external qualification blocker, not a passing check or a waiver.


## Rebase qualification

Rebased the four unchanged patches onto main `280aaa157` without merge commits;
`git range-diff` confirms patch equivalence. Main includes the separate advisory fix
from PR #840. On rebased head `d03ec70985b613dfc75eda4c642c1ee77ac211cb`, all
18 focused lifecycle tests, all 335 frontend test files, Rust CI (including the
advisory gate), and CodeRabbit passed. The new two-workspace browser regression
passed again against the real backend.

E2E build `1c278b8b-228d-4620-95c7-f0d6cf16fd6e` rejected its final receipt because
`workspace-review-recovery.spec.ts`'s ownership-transfer request-picker case needed
one retry. Its first attempt timed out after 60 seconds on the fixture's direct API
`POST /resume`, before the `parent-boot` setup checkpoint or test body. The second
attempt passed. Both general browser shards and all provider-agent test runs passed,
but the mandatory receipt correctly refused a retry-qualified result. The fixture
path is unchanged by this frontend PR; the new navigation test uses the shared
`deleteWorkspaceById` helper, which waits for deletion completion. No application
change is justified by this setup timeout. A fresh CI run is required; the failed
receipt is not treated as green. Deployed QA verification remains unestablished.
