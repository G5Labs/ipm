# Anyone who can read a workspace may start a comment thread

## Ontology changes and governance

**Comparison base:** `main` at `b0efd23d` (2026-10-15). Intent: [chat intent](../../chat_intent/comments-readers-start-threads/request.md).

**Ontology changes:** `g5.documents.comments.authority` in `features/documents.g5.md` now grants thread creation to every eligible reader, keeping resolve, reopen and delete with the owner and text edits with the author; its metadata becomes human-verified with the request and implementation lineage. No block is added.

```diff
--- a/.g5/ontology/features/documents.g5.md
+++ b/.g5/ontology/features/documents.g5.md
@@ [D · g5.documents.comments.authority]
-The workspace owner creates, resolves and deletes anchored threads; only the original author edits their text. Eligible workspace readers may reply, even while compute is stopped. Publish discussion events only after the write commits.
+Eligible workspace readers, the owner included, start anchored threads and reply, even while compute is stopped; the workspace owner resolves, reopens and deletes any thread, and only the original author edits their text. A thread is discussion, not a content write, so reading the workspace is enough to open one; deciding a thread stays with the one owner. Publish discussion events only after the write commits.
@@ ### g5.documents.comments.authority (metadata)
-- agent
+- human-verified
+- … the human's request (2026-10-15) replaced owner-only thread creation …
+- Implemented · `create_comment` and `add_reply` check read access; resolve/reopen/delete keep the owner check; both routes take read scope; the page derives `canStartThread` from a writable workspace alone. API, page and browser tests.
```

Relevant and governing ontology:

| Block | Why relevant | Implementation must |
|---|---|---|
| [g5.documents.comments.authority](../../ontology/features/documents.g5.md#g5.documents.comments.authority) | Owns who starts, decides and edits threads. | Admit any reader to start a thread; keep decide with the owner, edit with the author. |
| [g5.workspace.audience](../../ontology/features/workspaces.g5.md#g5.workspace.audience) | Defines who can read a workspace: owner, org administrators, org-visible members, invitees; former owners and creators keep nothing. | Check visibility and invitations in the service, since tenant row security admits any org member. |
| [g5.security.current-authority](../../ontology/concepts/security.g5.md#g5.security.current-authority) | A transactional mutation preserves the authority it relied on until it commits. | Hold the workspace row and the reader's invitation under a shared lock from the access check to commit (`AccessHold::UntilCommit`), for start, reply and author edit, as the owner path holds the workspace row. Organization membership is read from the system pool and is not held. |
| [g5.workspace.mutation-authority](../../ontology/features/workspaces.g5.md#g5.workspace.mutation-authority) | Only the current owner mutates workspace content. | A thread is a platform record, not repository content; the route needs read scope only and grants nothing else; deciding a thread stays with the owner under write scope. |
| [g5.workspace.base](../../ontology/features/workspaces.g5.md#g5.workspace.base), [g5.workspace.content-policy](../../ontology/features/workspaces.g5.md#g5.workspace.content-policy) | Base, and a workspace made read-only by its owner, refuse workspace-owned discussion writes. | Both keep refusing thread writes (`ensure_thread_writes_allowed`), unchanged. |
| [g5.documents.comments.drafts](../../ontology/features/documents.g5.md#g5.documents.comments.drafts) | A refused send keeps the words. | Unchanged; a reader's refused thread (read-only policy, Base) shows its reason in the draft. |

## Requested outcome and design

A reader of someone else's workspace sees the same Add comment controls as the owner (selection toolbar, context menu, Monaco glyph margin), sends the thread with no running VM, and the owner sees it live. Resolve, reopen and delete remain the owner's.

```
reader ──POST /workspaces/:id/comments (read scope)──▶ route ──▶ CommentService::create_comment
                                                                  ├─ ensure_user_workspace_access_until_commit  (visibility, invites; FOR SHARE to commit)  ← was ensure_workspace_owner
                                                                  ├─ ensure_thread_writes_allowed  (policy, Base; read under that hold)
                                                                  └─ insert + comment.new after commit
reader ──POST /comments/:id/replies (read scope)──▶ add_reply ──▶ same held access check, then policy under it
author ──PATCH /comments/:id {content} (read scope)──▶ update_comment ──▶ author ∧ same held access check, then policy under it
owner  ──PATCH {resolved} / DELETE /comments/:id (write scope)──▶ update/delete ──▶ ensure_workspace_owner (unchanged)
page   ── canStartThread = canReply = workspace accepts thread writes; canModerate = that ∧ owner
```

The change reuses the reply rule for creation: the read-access check the service already applied to replies now guards `create_comment`, and the route drops its write-scope gate the way `add_reply` already did. Because readers now author threads, a content edit by its author also takes read scope; the PATCH handler requires the credential's own write scope when `resolved` is sent (route middleware checks only the registered read scope, and the database layer checks grants and ownership, not the token), and the owner check stays in the service. The old owner path held the workspace row share-locked through commit; the reader path does the same for the workspace row and the invitation it relied on (review round 1), so the three reader writes cannot commit on access revoked meanwhile. The read-only policy is read after that hold on every reader path (CodeRabbit on #592, repaired in the follow-up), so a read-only transition either commits before the read and refuses the write, or waits behind it. No new state, table or permission flag. The page's permission object already separates start, reply and moderate, so only the derivation of `canStartThread` changes; every editor surface already renders its Add comment control from that one handler. The frontend has no control for editing one's own thread text, so the author-edit route change is API-only.

Acceptance: a reader with only read scope creates a thread (201) that the owner lists and resolves; the reader edits its text and nobody else does; the reader cannot resolve or delete their own thread (403), nor can a non-owner holding write scope; an uninvited member of an invite-only workspace is refused creation and reply alike; a revocation or audience change attempted while a reader's write is open waits for it (deterministic lock-timeout test) and the next write after it lands is refused; Base and a read-only-policy workspace still refuse; a former owner, still a reader, may start but not decide. The browser flow adds the reader starting a thread, expanded to show its reply box and no decisions, that the owner sees without reloading and resolves.

Risks and assumptions:

- A reader can open threads but never close their own; the owner moderates. The request named only starting. If readers should also resolve or delete what they started, that is a separate amendment to the same block.
- The read-only mutation policy and Base still refuse new threads. The request's "read-only workspace" was a reader's view of a normal workspace, so this reading is kept; the screenshot banner shows for both cases, which may be worth separating later.
- The static route audit (`route_auth_static.rs`) pins `create_comment` and `add_reply` to read scope with the ontology reason, so a later tightening is a deliberate change.

## Review summary

Reuse verified while coding: `ensure_user_workspace_access_until_commit` takes the workspace's org id, which the create path loads from the workspace row; a deleted workspace is refused as not found before any insert. The row-level policies on `workspaces` and `workspace_members` are plain org isolation and the tenant role holds UPDATE on both, so any member may take the shared locks. Remaining uncertainty: whether reader-started threads should notify the owner beyond @mentions (not requested; not added); organization-membership removal between admission and commit is not held (system-pool read), the same as every other admitted write.

Review round 1 (Codex, gpt-6-astra): P0 access revocable before commit, P1 reader-authored text uneditable, P2 two assertions passing on collapsed or hidden controls, P2 stale ontology links. Round 2: P1 an owner's read-only credential could resolve through the write-scoped database path. All repaired above. Record: `.g5/ontology-review-comments-readers-start-threads.md`.
