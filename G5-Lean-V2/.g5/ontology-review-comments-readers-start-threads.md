# Review record: readers start comment threads

Governing block: [`g5.documents.comments.authority`](ontology/features/documents.g5.md#g5.documents.comments.authority), with [`g5.security.current-authority`](ontology/concepts/security.g5.md#g5.security.current-authority) and [`g5.workspace.audience`](ontology/features/workspaces.g5.md#g5.workspace.audience). Design: [ontology-plan](pr_ontology_diff_design/comments-readers-start-threads/ontology-plan.md). Branch `kaihui/comments-readers-start-threads`, draft PR #592.

Independent reviews run locally with Codex (`gpt-6-astra`, reasoning xhigh, read-only sandbox) against the worktree diff from merge base `b0efd23d`. Each round's report is kept in the job's scratch directory; this record keeps the findings and their repairs.

## Round 1 — REQUEST_CHANGES

| Severity | Finding | Repair |
|---|---|---|
| P0 | Read access could be revoked between the service's access check and commit: the old owner path held the workspace row `FOR SHARE` through commit, the new read-scoped path held nothing. | `scoped.rs` gains `AccessHold::UntilCommit` and `ensure_user_workspace_access_until_commit`: the workspace row and, for an invitee, their `workspace_members` row are share-locked on the write transaction. Used by `create_comment`, `add_reply` and the content branch of `update_comment`. Test `test_a_readers_thread_write_holds_its_access_until_commit`: a revocation and an audience change attempted while the write is open time out on the lock; after commit the revocation lands and the next write is refused. Organization-membership removal (system-pool read) is not held; stated in the plan. |
| P1 | A reader-authored thread's text was editable by nobody: PATCH needed write scope, which the comment-scoped DB admits only for the owner, so the service's author check was unreachable. | PATCH registers with read scope; the handler escalates to write scope only when `resolved` is sent; the service keeps author-only text edits and owner-only decisions. Tests: the author edits with read scope; the owner and another reader are refused; a write-scoped non-owner cannot resolve or delete but can start. The UI has no own-text edit control, so the fix is API-only. |
| P2 | The browser flow asserted no Resolve on the reader's new thread while it was collapsed, where decisions never render. | The flow expands the thread, asserts its reply box is visible and Resolve and Delete absent, then has the owner expand and resolve it and the reader see the decision. |
| P2 | The page test toggled the comments margin closed after starting the thread, then exercised hidden controls. | The toggle is removed; the test asserts the thread and its reply box are visible. |
| P2 | The plan's governing table linked to ontology IDs that no longer exist. | Rows now cite `g5.workspace.audience`, `g5.security.current-authority`, `g5.workspace.mutation-authority`, `g5.workspace.base` and `g5.workspace.content-policy`. |

Verified by the reviewer and left as is: membership, tenant isolation, visibility, invitation and deleted-workspace checks reject unauthorized creation; decisions keep owner checks; Base, read-only policy and freezing guards precede insertion; events and mentions publish after commit; SpecEditor and CodeEditor controls take the reader's handler; the static route audit matches; no CLI creation path.

Evidence after repair: `api_comments` and `route_auth_static` 44/44 locally; `WorkspacePageComments.test.tsx` reader tests pass; `tsc` clean; the Playwright spec lists; clippy and rustfmt clean.

## Round 2 — REQUEST_CHANGES

| Severity | Finding | Repair |
|---|---|---|
| P1 | With PATCH registered under read scope, passing `workspaces:write` to the comment-scoped DB checked database grants and ownership but not the credential's own scope ceiling (route middleware owns that, and it now checked read only), so an owner's read-only API key could resolve or reopen a thread. | The handler refuses a `resolved` change unless the credential carries `Scope::WorkspacesWrite`, with the middleware's `missing_scopes` shape, before opening the write-scoped transaction. Test: the owner's read-only token is refused for resolve, for a reopen combined with an edit, and for delete, and succeeds for an author-only text edit. |

Verified by the reviewer: the held-access helper locks the workspace before any invitation with no opposing lock order in transfer, visibility, invitation or owner-check paths; the lock clause is a literal, identifiers stay bound; deleted workspaces are refused; combined `content` and `resolved` requests run both service checks; separate PATCH and DELETE registrations apply their own scope layers; Base, read-only policy, freezing and after-commit publication intact; the repaired frontend tests exercise visible, expanded threads.

## Round 3 — APPROVE

No findings. Verified: the credential's write scope is required before any `resolved` change and content-only edits keep author and held read-access checks; `create_comment` and `add_reply` pass read scope, `delete_comment` write scope, matching their registrations; API keys keep their own scopes, host tokens are rejected on public routes, incarnation and capability credentials cannot stand in for user tokens; the static audit pins the four registrations; the owner's read-only-token test isolates credential scope from database authority; frontend permissions, design and ontology agree on reader creation and owner moderation.

Converged after three rounds: each earlier round found a new class of defect (held authority, author reachability, credential scope ceiling) rather than a repeat shape.

## CodeRabbit (on the pushed PR)

| Severity | Finding | Repair |
|---|---|---|
| Major | PATCH required write scope, so a reader-author could not edit their text. | Already repaired in round 1; CodeRabbit marked it addressed. |
| Minor, outside diff | In `create_comment` and the content branch of `update_comment`, the read-only policy was read before the shared lock was taken, so a read-only transition could commit in between; `add_reply` already checked policy after the hold, and the owner paths hold the row from transaction open. | The policy check moves after each path's hold (follow-up PR #596). The lock-timeout test also shows a read-only transition waiting on an open reader write, and `test_a_readers_thread_write_sees_a_transition_committed_while_it_waited` distinguishes the orderings: with a transition already in flight, a new thread and an author's edit wait on the row and are then refused as read-only, where the old order would have proceeded on its stale policy read. |

## Follow-up PR #596 — Codex round 1 — APPROVE

One P3: the plan's creation diagram still drew the policy check before the hold; swapped. The reviewer noted the extended lock-timeout assertion does not distinguish the orderings and described the test that would; it is added as above. Verified: policy is read on the same transaction as the hold, so a committed transition is seen and an in-flight one waits; the transition path takes `FOR UPDATE` on the workspace row, which conflicts with the hold; resolved-only, combined and delete paths are covered by their owner locks; error precedence changes are bounded and no test or client depended on the old one; no new lock-order cycle.
