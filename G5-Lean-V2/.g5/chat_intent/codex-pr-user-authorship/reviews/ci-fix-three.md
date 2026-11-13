# CI and review fixes — independent incremental review three

Scope: current uncommitted changes against `530dc8fb`, including all changed
production/test consumers and the pinned plan's CI and review follow-up. This reviewer
did not author these fixes; its earlier CLI review context concerns a different
increment. Read AGENTS.md, the request/design, ontology-review and design-review
guidance, current governing contracts/metadata, `/tmp/g5-pr268-review-comments.json`
and `/tmp/g5-pr268-ci-rust-failures.log`. No peer findings or author follow-up in
`review.md` were consulted before saving this report. The supplied review comments
explicitly identify timeout visibility, duplicate scans, hidden target-query errors,
migration references and an obsolete profile-size concern.

No production, test, or ontology changes made. No delegated work or heavy tests;
the main agent owns test execution. Findings below are source-inspected counterexamples,
not claimed runtime reproductions.

## Initial findings

### F1 — Failure recording repeats potentially stuck work outside the deadline (P2)

`backend/g5-api-core/src/services/workspace_pull_request_create.rs:527–530` invokes
`needs_push_refresh` after the attempt's timeout has elapsed, without any second
deadline. When no pending flag/provider mismatch exists yet, that call reaches
`published_heads` → `resolve_published_snapshot` → `read_branch_head_oids`, including
an awaited blocking filesystem/Git ref read (`workspace_reader.rs:1231–1263`).

Thus a first refresh that times out while reading published heads can immediately
repeat the same stuck storage read in its error-recording phase while holding the
source and PR locks. `SET LOCAL lock_timeout='5s'` bounds PostgreSQL lock waits; it
does not bound this I/O. The scan now waits on the entire refresh without its old
outer timeout, and the persistent scan leader prevents other replicas from making
progress. The new timeout test covers a transient PR-row lock that is released;
it does not discriminate this branch. This conflicts with
`g5.architecture.recovery.progress` and leaves deadline failure invisible in exactly
this case.

Bound error reconciliation as well, or avoid repeating potentially stalled Git reads
when recording the deadline, using appropriately fenced/versioned database state to
avoid overwriting newer success. Add an injected stalled-head-read case that times
out, leaves durable pending/error evidence where authoritative recording is possible,
and lets a later eligible PR progress. Preserve the current stale-owner and
terminal/completed-PR guards.

### F2 — Stopped owners still get a disabled PR action with no destination error/retry (P2)

The new banner at `frontend/src/pages/WorkspacePage.tsx:10402` requires
`!workspaceReadOnly`. That value includes `!ownerHasRunningAttachment`
(`WorkspacePage.tsx:5835–5839`), so it is true for an ordinary stopped workspace
owned by the signed-in user. The ReposBar Create PR action has no corresponding
running-incarnation requirement (`WorkspacePage.tsx:10364–10377`): a failed target
query still leaves its empty-target disabled button, while the new banner disappears.

PR publication from retained stopped state is supported by the shared operation
(including `create_pr_does_not_omit_dirty_stopped_flush_snapshots`), and opening/retrying
the target query is a durable read. Therefore a stopped owner still encounters the
exact inaccessible-error condition called out by the review. The added UI test uses
the running/default VM fixture and misses this state.

Gate the banner on the durable PR-action eligibility/owner state rather than live-edit
eligibility, consistently with the existing Create PR control. Add a stopped-owner
target-query failure/retry case, with no VM start, alongside the current running-owner
case. This preserves the shared target authority contract and the design's visible
failure/retry requirement.

## Coverage without an additional finding

| Governing commitment / requested repair | Inspected path and assessment |
|---|---|
| `g5.governance.pr.write`, `g5.architecture.pr_flow`, current source/target authority | Public route remains a thin `PullRequestCaller::User(&auth)` adapter into `create_or_update`; source write scope, mutation/owner checks and target read admission precede preparation. No duplicated production flow or newly bypassed authorization found. |
| Static audit strength, `g5.quality.review.guards` | New exception is limited to the exact handler/path argument and requires typed authenticated delegation plus `params.id`. Calibration rejects removing either input; companion checks inspect authorization functions and ordering. This is a lexical structural audit, not a proof of arbitrary control-flow/error propagation. No unconditional exemption added. |
| `g5.governance.pr.follow-push`, visible failure, `g5.design.commit-notify` | Timeout is converted into a sanitized error. After fresh source-owner admission, PR row locking and completion checks, recording commits before notification. SQL lock waits remain bounded. F1 is the remaining bound outside this intended sequence. |
| Current ownership, terminal PRs, newer completed refresh | Failure path obtains a fresh scoped caller, checks/locks current source owner, locks PR row, skips non-open and currently complete PRs, and conditions the update on current owner/source. No write using the captured old owner without re-admission found. These race cases are inspection-only here. |
| Duplicate scans / leadership lifecycle | Dedicated PostgreSQL session connection holds the advisory lock across loop ticks; followers skip scanning; cancel drops the connection; failed ping discards leadership before the next scan. No pooled connection retains a session lock. Existing per-PR locks continue to authorize effects. The new takeover test covers steady ownership and cancel between scans. Connection loss during an in-progress scan and cancellation during provider I/O are not independently executed. |
| UI target retry | Alert is outside ReposBar/status-band layout visibility gates; query invalidation uses the same key as `useWorkspacePullRequestTargets`. Running owner failure → retry → restored Prepare-to-Ship action has a meaningful mocked HTTP test. F2 remains for stopped owners. |
| Architecture/minimality | Shared refresh owner retains deadline/error policy; maintenance owns leader/scanning state; no new service, route, credential or publication grant added. The dedicated connection avoids consuming a normal pool slot for the leader lifetime. |
| Migration references / obsolete profile comment | Embedded current references change to migrations 046/047, matching current governance metadata. No unnecessary profile rewrite included. |

## Verification limits

No tests or builds executed by this reviewer, per assignment. Existing and added tests
were inspected; results must come from the main agent's current checkout run. No live
GitHub, multiple deployed API replicas, database connection-loss injection, stalled
filesystem, real provider timeout/cancellation, or browser runtime reproduction. The
new loop's normal leadership test does not itself establish all cancellation/network
partition behavior. Ontology metadata should retain these limits and record any
confirmed F1/F2 repair evidence after consolidation.
