# Anyone who can read a workspace may start a comment thread

Branch `kaihui/comments-readers-start-threads`; comparison base `main` at `b0efd23d` (2026-10-15).

## Human instructions

Request (2026-10-15), with two screenshots: a read-only view of another person's workspace whose selection toolbar has no Comment button, and the owner's live workspace whose toolbar shows one.

> In a read-only workspace the Comment control does not show, so only the owner, and only with a VM, can comment. That is wrong; anyone should be able to click Comment. Can you investigate and fix it?

## Agent interpretation

The screenshot's "read-only workspace" is another person's workspace seen by a reader, not one whose owner chose the read-only mutation policy. The page offered Add comment only to the current owner, and the server refused a reader's new thread, so a reader could reply in existing threads but never open one. The VM was never part of the rule: the owner starts threads while stopped.

"Anyone" is read as anyone who can read the workspace, the same grant readers already have for replies. Deciding a thread (resolve, reopen, delete) stays with the current owner, and the read-only mutation policy and Base keep refusing thread writes; the request did not ask to change either. Nothing else about comments changes.
