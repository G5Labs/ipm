# Comments and review-suggestion threads

Agent review comments, attribution and round tools described in this historical record
were retired by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md). They are not supported interfaces.

## Ontology changes and governance

**Comparison base:** `origin/main` at `a1c3e29e`, branch `zcl/comments-design`. The request, and the defects it refers to, are in the [chat intent](chat_intent/comments-design/request.md).

**Ontology changes:**

- **Added** in [documents](ontology/features/documents.g5.md): `g5.documents.comments.identity`, `.authority`, `.replies`, `.controls`, `.failure`, `.live`. `.replies` records the human's answer to this design's first open question.
- **Added** in [governance](ontology/features/governance.g5.md): `g5.story.discuss-a-review-suggestion`, `g5.governance.review-suggestions`, `g5.governance.review-suggestions.display`, `g5.governance.review-thread`, `.review-thread.access`, `.review-thread.attribution`, `.review-thread.notices`.
- **Added** in [Git/governance module](ontology/software_architecture/git_governance.g5.md): `g5.architecture.review_owner`.
- **Meaning changed:**
  - `g5.documents.comments`: the viewed bytes include unsaved edits.
  - `g5.documents.comments.reanchor`: evidence is tried in order, and context is compared per side.
  - `g5.documents.comments.ambiguity`: legacy rows with no anchor are shown as unanchored.
  - `g5.governance.review-workspace`: suggestion hand-back replaces PR hand-back.
  - `g5.architecture.comment_owner`: owns who may act, is the only writer, and keeps review threads apart.
- **Links only:** `g5.story.discuss-a-precise-line`, `g5.story.return-a-review-change`, `g5.story.accept-review-changes`, and the [story index](ontology/concepts/core_user_stories.g5.md).
- **Metadata only:** `g5.governance.review-workspace.finish` (the Finish operation no longer exists) and `g5.scope.review-product` (partial reassessment).
- **Removed:** none.
- **Amended after the implementation review** (see [the review](ontology-review-comments.md), finding K2): `g5.documents.comments.reanchor` gains a paragraph saying when a recorded context side is evidence. An empty side (the file's edge), blank lines alone (including the empty line after a final newline) and a side shortened to fit the size bound identify nothing; a side cut short by the file's start or end is compared as recorded. The handoff wording ("an empty or size-truncated side counts as no evidence") let an end-of-file comment jump to a twin through the phantom final line. Metadata of the changed blocks and of `g5.design.commit-notify`, `g5.architecture.notification_owner`, `g5.notifications.durable` and `g5.workspace.identity.base` now describe the implementation.

- **Added during the implementation review**, each by a human decision recorded in the [chat intent](chat_intent/comments-design/request.md), and recorded under the amendment rule:
  - `g5.workspace.page.refresh` in [workspaces](ontology/features/workspaces.g5.md#keep-the-workspace-page-through-a-failed-refresh), round 5. The workspace page, and what is typed in it, stays as last loaded through transient refresh failures; a refusal still shows the error.
  - `g5.security.login.principal` in [security](ontology/concepts/security.g5.md#act-for-one-principal-per-browser-tab), round 7. A tab acts for one principal; adopting another's token clears what the previous one typed and sends none of their requests under the new token. Its metadata records two gaps left to follow-up tasks: cached server data is not yet split by principal (deferred by the human in round 8), and actions outside comment and review threads are not bound to the page's principal.
  - The metadata of `g5.documents.comments`, `.controls`, `.failure` and `g5.governance.review-suggestions.display` follows each round's repairs.

The full diff at the handoff is at the end of this section. The K2 amendment adds only the reanchor paragraph above to it; the two blocks added during the review are in their files.

**Relevant and governing ontology:**

| Commitment | Obligation → owner and operation | Allowed case · counterexample | Conclusion |
| --- | --- | --- | --- |
| [g5.documents.comments](ontology/features/documents.g5.md#a-comment-keeps-its-original-meaning-when-code-moves) | The editor that displays the text builds the anchor from exactly that text; the page and CommentService store it unchanged. | A comment on a block below three unsaved new lines anchors to that block · hashing the last loaded version. | Changed; code gap. |
| [g5.documents.comments.identity](ontology/features/documents.g5.md#a-files-comments-follow-the-file) | Every document, raw-file and ontology view lists and creates threads by workspace, repository and path. CommentService lists without a branch filter. | A comment made in a raw-file tab appears in the document view of the same file · listing by tab type, node id or branch. | Added. Open assumption: the ontology content read exposes the node file's repository and path. |
| [g5.documents.comments.reanchor](ontology/features/documents.g5.md#reanchor-only-with-evidence-of-the-same-content) | One shared resolver serves both editors and applies the evidence in order. | The comment follows its paragraph after 40 lines are inserted above, and after the paragraph above is edited · it jumps to an identical heading 61 lines away (today). | Changed; code gap. |
| [g5.documents.comments.ambiguity](ontology/features/documents.g5.md#keep-ambiguous-comment-anchors-visible) | An unmatched thread shows its original line and anchored text; legacy node rows are marked unanchored. | "Was line 40: …" · a bare "Orphaned anchor" label. | Changed; code gap. |
| [g5.documents.comments.authority](ontology/features/documents.g5.md#the-current-owner-writes-comments) | CommentService decides. The current owner starts, resolves, reopens or deletes any thread, and a person edits only their own messages; no VM is needed and a read-only workspace refuses. | A new owner resolves the previous owner's thread, and a stopped owner comments · a reader starts or resolves a thread, or a new owner is refused. | Added; codifies H072. |
| [g5.documents.comments.replies](ontology/features/documents.g5.md#let-readers-reply-in-threads) | Anyone who can read the workspace replies in either thread kind, with no VM, through CommentService (anchored) or the review owner (suggestion). | A mentioned teammate answers in the thread · a reply that resolves the thread, or a reply accepted on a read-only workspace. | Added; human answer. |
| [g5.documents.comments.controls](ontology/features/documents.g5.md#offer-only-permitted-comment-actions), [.failure](ontology/features/documents.g5.md#keep-a-persons-words-when-a-comment-action-fails) | Comment surfaces show only permitted actions, and on refusal or failure they explain and keep the typed text. | A reader sees Reply but no Add comment, Resolve, Reopen or Delete; a refusal shows its reason with the text intact · the draft closes and its text is gone. | Added. |
| [g5.documents.comments.live](ontology/features/documents.g5.md#show-thread-changes-to-other-viewers) | CommentService publishes an event for every thread change after commit, and clients refetch on receipt. | A reader sees a resolve without reloading · an event before commit, or none for a delete. | Added. |
| [g5.architecture.comment_owner](ontology/software_architecture/git_governance.g5.md#comment-anchors-and-replies), [g5.architecture.review_owner](ontology/software_architecture/git_governance.g5.md#review-hand-back) | CommentService is the only writer of thread and reply rows. Review decides its own thread authority and calls CommentService inside its transaction from every entry path. | A human reply through the review route passes the same checks · a generic `/comments/:id` reply lands in a suggestion thread. | Changed and added. |
| [g5.governance.review-suggestions](ontology/features/governance.g5.md#a-review-returns-suggestions-to-the-source-owner) | Push review returns suggestions as records on the source; recording a round moves no Git ref. | Recovered as shipped by PR #170 · a pull-request hand-back. | Added. Open assumption: the scope reading of H081. |
| [g5.governance.review-suggestions.display](ontology/features/governance.g5.md#show-a-suggestion-the-same-way-everywhere) | The Reviews panel and file views offer the same actions and thread; a suggestion is never hidden or presented as a match. | A reviewer withdraws from the in-file card, and a card in a collapsed section is marked unplaced · a silently dropped card (SpecEditor today). | Added; code gap. |
| [g5.governance.review-thread](ontology/features/governance.g5.md#discuss-a-suggestion-in-its-thread), [.access](ontology/features/governance.g5.md#let-every-reader-join-a-review-thread) | One thread per suggestion; any reader of the source replies through the review owner; decisions stay with their roles. | A teammate replies after a reject · a generic delete detaches the thread, or a reply is counted as a decision. | Added; access recovered. |
| [g5.workspace.page.refresh](ontology/features/workspaces.g5.md#keep-the-workspace-page-through-a-failed-refresh) | The page's queries keep their last loaded value on a transient failure and say so; refusals still show the error; approvals stay fail-closed. | A 503 on the workspace detail keeps a typed reply and an open draft · the page swapping to its error screen and dropping them. | Added during the review (round 5, human decision). |
| [g5.security.login.principal](ontology/concepts/security.g5.md#act-for-one-principal-per-browser-tab) | The auth provider retires the page when another principal's token is adopted; the API client sends a request only for the principal it was made for. | A new comment submitted as Alex is refused when Bob's token arrives first · Alex's words posted under Bob's token. | Added during the review (round 7). Gaps: cached server data is not split by principal (deferred, round 8); actions outside comment and review threads are not bound to the page's principal. |
| [g5.governance.review-thread.notices](ontology/features/governance.g5.md#notify-the-other-party-once) | The review owner notifies the other party once per reply or decision, never the actor, after commit. | Owner resolves, so the reviewer gets one notice · reviewer = owner gets a self-notice. | Added; code gap. |
| [g5.security.read](ontology/concepts/security.g5.md#read-permission-is-explicit), [tenant](ontology/concepts/security.g5.md#tenant-scope-survives-privileged-operations), [tenant.rls](ontology/concepts/security.g5.md#enforce-tenant-scope-inside-database-access) | Readers read threads; events and records keep tenant scope. The unread `comment_mentions` table is removed, or put under forced RLS. | A reader of an org-visible workspace sees its threads · a tenant-role query reads other orgs' mention rows. | Preserve; fix the mention-table gap. |
| [g5.security.tenant.write](ontology/concepts/security.g5.md#separate-membership-from-write-permission), [g5.architecture.policy](ontology/concepts/system_design_principles.g5.md#share-policy-without-erasing-legitimate-exceptions), [g5.security.read.cache](ontology/concepts/security.g5.md#read-membership-on-every-check) | Replies are one shared, human-chosen grant for both thread kinds, and grant no workspace-content write. Thread actions stay owner-only. A revoked reader is refused on their next request: membership is read on every check since the cache removal. | A reader replies · a reader edits a file or starts an anchored thread. | Preserve. |
| [g5.security.admission](ontology/concepts/security.g5.md#admit-actions-against-current-authority), [g5.lifecycle.transfer](ontology/software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge), [g5.workspace.identity.base](ontology/features/workspaces.g5.md#keep-base-workspaces-as-protected-readers) | Comment writes are durable actions: no VM gate. Authority moves at transfer; Base refuses writes. | A stopped owner comments · a Base comment is accepted. | Preserve. Open assumption: Base refusal is not yet inspected. |
| [g5.design.commit-notify](ontology/concepts/system_design_principles.g5.md#publish-notifications-after-authoritative-commit), [g5.notifications.durable](ontology/features/notifications.g5.md#recover-notifications-after-disconnecting), [g5.architecture.notification_owner](ontology/software_architecture/platform.g5.md#shared-notification-delivery) | Comment events and notices are published only after the governing transaction commits. The fix belongs in the shared producer, for every caller. | A subscriber that refetches on the event sees the row · a notice for a rolled-back reply. | Preserve; fix at the producer. |
| [g5.design.enforcement](ontology/concepts/system_design_principles.g5.md#enforce-boundaries-on-every-entry-path), [g5.architecture.commands](ontology/software_architecture/boundaries.g5.md#keep-complete-operations-with-their-state-owner), [g5.design.transaction-composition](ontology/concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects), [g5.design.adapters](ontology/concepts/system_design_principles.g5.md#keep-adapters-free-of-domain-policy) | Owners decide authority and write their own rows; routes translate requests. | CommentService joins the review transaction · a route-only ownership check, or raw review SQL. | Preserve. |
| [g5.documents.editor-performance](ontology/features/documents.g5.md#keep-large-documents-usable), [g5.documents.save.disk-change](ontology/features/documents.g5.md#expose-concurrent-disk-changes) | Re-anchoring must not slow typing or flash wrong labels. Anchors follow the displayed buffer, including a dirty one. | Resolution recomputes after edits without blocking input · every keystroke rehashes all lines and briefly marks all threads orphaned (today). | Preserve. |
| [g5.workspace.identity.branch](ontology/features/workspaces.g5.md#bind-member-branches-to-one-workspace-name), [g5.fork.source.identity](ontology/features/workspaces.g5.md#give-the-fork-its-own-lineage-identity) | One branch per member makes the branch a label; a fork or review copy starts without the source's anchored comments. | Comments survive a rename of the workspace branch · comments stranded under `main`. | Preserve. |
| [g5.design.evolution](ontology/concepts/system_design_principles.g5.md#evolve-contracts-without-corrupting-existing-state), [g5.quality.migrations](ontology/concepts/coding_principles.g5.md#preserve-deployed-data-through-forward-migrations), [.verification](ontology/concepts/coding_principles.g5.md#validate-migrations-as-the-real-migration-role) | Use forward migrations for reply authorship and the mention table; legacy rows stay valid. | Existing replies read as person-written · editing an old migration. | Preserve. |
| [g5.quality.review](ontology/concepts/coding_principles.g5.md#review-the-changed-contract-and-its-consumers), [.cases](ontology/concepts/coding_principles.g5.md#review-both-permitted-and-forbidden-behavior), [.guards](ontology/concepts/coding_principles.g5.md#calibrate-structural-guards-against-harmless-changes), [g5.quality.evidence](ontology/concepts/coding_principles.g5.md#test-real-boundaries-and-report-exact-evidence) | Tests cover allowed and forbidden paths under the restricted runtime role. Route-auth guards are recalibrated when checks move into owners. | Reader 201 on an anchored and a review reply, and 403 on starting, resolving, reopening or deleting a thread · a guard that passes only because a check disappeared. | Preserve. |
| [g5.scope.review-product](ontology/scope.g5.md#ordinary-reviews-remain-ordinary-workspace-work), [g5.external-issues.discussion](ontology/features/external_issues.g5.md#keep-shared-discussion-at-the-primary-issue) | Neither a returned-result product nor a new comment store is introduced. | Existing tables only · a third thread store. | Preserve; the H081 reading is for the human to confirm. |

<details>
<summary>Actual ontology diff at the handoff (<code>git diff -U0</code> against the comparison base)</summary>

```diff
diff --git a/.g5/ontology/concepts/core_user_stories.g5.md b/.g5/ontology/concepts/core_user_stories.g5.md
index aedd8c02..cff3e9f2 100644
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -138,0 +139 @@ These links add no separate commitments or implementation percentages.
+- [Discuss a review suggestion](../features/governance.g5.md#discuss-a-review-suggestion).
diff --git a/.g5/ontology/features/documents.g5.md b/.g5/ontology/features/documents.g5.md
index 2b4c663c..4896443b 100644
--- a/.g5/ontology/features/documents.g5.md
+++ b/.g5/ontology/features/documents.g5.md
@@ -78 +78 @@ Governing concepts: [Read permission](../concepts/security.g5.md#read-permission
-Governing contracts: [a comment keeps its original meaning when code moves](documents.g5.md#a-comment-keeps-its-original-meaning-when-code-moves).
+Governing contracts: [a comment keeps its original meaning when code moves](documents.g5.md#a-comment-keeps-its-original-meaning-when-code-moves), [a file's comments follow the file](documents.g5.md#a-files-comments-follow-the-file), [the current owner writes comments](documents.g5.md#the-current-owner-writes-comments), [let readers reply in threads](documents.g5.md#let-readers-reply-in-threads).
@@ -271 +271,9 @@ This repository’s local I/D/Q ontology does not establish a shipped parser or
-Anchor a line comment to the actual viewed bytes using repository/path, available base OID, content/line hashes and bounded context.
+Anchor a line comment to the actual viewed bytes using repository/path, available base OID, content/line hashes and bounded context. The viewed bytes are the text the editor is displaying, unsaved edits included; a saved or committed version never stands in for them.
+
+### A file's comments follow the file
+
+[D · g5.documents.comments.identity]
+
+A comment belongs to one line of one file, identified by workspace, repository and path. Every surface showing that file — document, raw file or ontology view — lists and creates the same threads. The branch name is a label that never selects or hides a thread, and there is no separate node-level comment.
+
+[One branch per workspace member](workspaces.g5.md#bind-member-branches-to-one-workspace-name) makes the branch redundant as identity.
@@ -277 +285 @@ Anchor a line comment to the actual viewed bytes using repository/path, availabl
-Reanchor only through verified equal content, applicable hunk mapping or a unique hash-and-context match within 20 lines.
+Reanchor only on evidence that a line is the commented one, accepted in this order: verified equal content; applicable hunk mapping; the only line in the file matching the line hash with the recorded context on both sides; the only line within 20 lines of the recorded position matching the line hash with the recorded context on one side.
@@ -284 +292 @@ Reanchor only through verified equal content, applicable hunk mapping or a uniqu
-Show an orphan or ambiguous comment with its original anchor instead of silently choosing the nearest duplicate.
+Show an orphan or ambiguous comment with its original anchor instead of silently choosing the nearest duplicate. A comment stored without a line anchor stays readable and answerable where its file or node is shown, marked as unanchored.
@@ -287,0 +296,34 @@ Governing concepts: Repository content follows [Git authority](../concepts/data_
+### The current owner writes comments
+
+[D · g5.documents.comments.authority]
+
+Only the current workspace owner starts, resolves, reopens or deletes a thread — any thread in the workspace, whoever wrote it. A person edits only messages they wrote. These are durable record actions that need no running VM; a read-only workspace, [Base](workspaces.g5.md#keep-base-workspaces-as-protected-readers) included, accepts none.
+
+This applies [durable-action admission](../concepts/security.g5.md#admit-actions-against-current-authority) and the [write grant](../concepts/security.g5.md#separate-membership-from-write-permission); [transfer](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge) moves this authority with ownership. [Replies](#let-readers-reply-in-threads) follow their own rule.
+
+### Let readers reply in threads
+
+[D · g5.documents.comments.replies]
+
+Anyone who can read a workspace may reply in its threads, anchored comments and [review suggestions](governance.g5.md#discuss-a-suggestion-in-its-thread) alike, with no running VM. A reply changes no repository content and grants no other action; a read-only workspace accepts no replies.
+
+The grant follows [read permission](../concepts/security.g5.md#read-permission-is-explicit). It is a human-chosen exception to [owner-only thread actions](#the-current-owner-writes-comments) and confers no workspace-content write.
+
+### Offer only permitted comment actions
+
+[D · g5.documents.comments.controls]
+
+Offer a comment action only to a viewer who may perform it.
+
+### Keep a person's words when a comment action fails
+
+[D · g5.documents.comments.failure]
+
+A refused or failed comment action says why and keeps what the person typed.
+
+### Show thread changes to other viewers
+
+[D · g5.documents.comments.live]
+
+When a thread is started, answered, resolved, reopened, edited or deleted, other people viewing that file see it without reloading. Events follow [notify after commit](../concepts/system_design_principles.g5.md#publish-notifications-after-authoritative-commit); the stored threads remain the authority after a reconnect.
+
@@ -478 +520 @@ Partial incorporation does not terminalize a future contribution session.
-- ~75% · comment_service.rs and comment UI; byte/hash/context anchoring exists; hunk mapping and the nearby-match bound are missing, and original-anchor rendering is only partly assessed.
+- Partial · `comment_service.rs`, `frontend/src/lib/commentAnchors.ts` and the comment UI store and resolve hash anchors, but the gaps recorded under the comment contracts (unsaved-text anchoring, per-tab identity, re-anchoring, original-anchor display) keep a comment from reliably holding its line. Source-inspected; anchor cases replayed against the resolver, not run in a browser.
@@ -639,2 +681,8 @@ Partial incorporation does not terminalize a future contribution session.
-- From [C138](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md).
-- ~85% · `comment_service.rs` create DTOs and stored anchors; complete dirty-buffer/base-OID and reader rendering behavior is unassessed.
+- From [C138](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md); the unsaved-text sentence restates v9 §9.3 for the [comments design](../../chat_intent/comments-design/request.md).
+- Partial · `comment_service.rs` stores the anchor fields, but `frontend/src/pages/WorkspacePage.tsx::handleAddComment` hashes the last loaded version while `SpecEditor.tsx` and `CodeEditor.tsx` count the line in the unsaved buffer, so with unsaved edits the comment is dropped or lands on another line. Hashes are client-supplied and only shape-checked. Replayed against `commentAnchors.ts`; not browser-verified.
+
+### g5.documents.comments.identity
+
+- agent
+- Agent decision in the [comments design](../../chat_intent/comments-design/request.md); [v9 §9.3](../../../docs/lean-backend-design-v9.md) already treats `origin_branch` as a label.
+- Gap · `WorkspacePage.tsx` lists line comments only on document tabs, filtered by branch name, and node comments everywhere else: raw file tabs store comments they never list, and ontology tabs store node-level rows without anchors. `comment_service.rs::list_comments` filters on `origin_branch` and `create_comment` still accepts node scope.
@@ -645,2 +693,2 @@ Partial incorporation does not terminalize a future contribution session.
-- From [C138](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md).
-- ~60% · `frontend/src/lib/commentAnchors.ts`; hash/context matching exists, but hunk mapping and proximity restriction are absent.
+- From [C138](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md). The both-sides and one-side context wording is an agent refinement in the [comments design](../../chat_intent/comments-design/request.md): read literally, v9's single hash-plus-context rule orphans a comment when a neighbouring line is edited or a long section is inserted above it.
+- Gap · `frontend/src/lib/commentAnchors.ts::resolveCommentAnchor` accepts a lone hash match anywhere in the file with no context or distance check and uses context only to break ties; hunk mapping is absent.
@@ -651,2 +699,32 @@ Partial incorporation does not terminalize a future contribution session.
-- From [C138](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md); outcome lineage: [v9](../../../docs/lean-backend-design-v9.md) §9.3.
-- ~75% · comment anchor resolver exposes ambiguity; original-anchor rendering across every viewer remains partly unassessed.
+- From [C138](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md); outcome lineage: [v9](../../../docs/lean-backend-design-v9.md) §9.3. The unanchored sentence is an agent decision in the [comments design](../../chat_intent/comments-design/request.md) for rows stored at node level or before hash anchors.
+- Partial · the resolver reports orphan and ambiguous, but `frontend/src/components/comments/CommentThread.tsx` shows only a label without the original line or text, and rows without hashes are labelled orphaned.
+
+### g5.documents.comments.authority
+
+- agent
+- Codifies the implemented single-writer rule ([H072](../../uplift/g5-lean/candidates/human-decisions.md#h072--one-owner-writes-transfer-replaces-multi-writer-sharing)) for starting and deciding threads; owner-wide resolve and delete and the no-VM rule are agent decisions in the [comments design](../../chat_intent/comments-design/request.md), where the human separately chose reader replies.
+- Partial · `backend/g5-api-routes/src/routes/comments.rs` requires the workspace owner for every write, replies included, and refuses a read-only workspace. `comment_service.rs::update_comment` and `delete_comment` also require the author, so after a transfer nobody can resolve or delete earlier threads. The editors offer a new comment only while the VM is attached. Whether Base is refused was not inspected.
+
+### g5.documents.comments.replies
+
+- human-verified
+- The human chose reader replies with no VM in the [comments design](../../chat_intent/comments-design/request.md), answering its open question; review threads have allowed them since [PR #170](https://github.com/G5Labs/G5-Lean/pull/170). Read-only refusal and the no-other-action limit are agent wording.
+- Gap · `routes/comments.rs::add_reply` requires the workspace owner and write scope; `routes/workspace_reviews.rs::add_reply_as` already admits any reader of the source with read scope.
+
+### g5.documents.comments.controls
+
+- agent
+- Agent decision in the [comments design](../../chat_intent/comments-design/request.md).
+- Gap · `CommentThread.tsx` offers Resolve and Reopen to every viewer and limits Delete to the author; it offers Reply to everyone, which the server refuses for readers until reader replies land.
+
+### g5.documents.comments.failure
+
+- agent
+- Agent decision in the [comments design](../../chat_intent/comments-design/request.md).
+- Gap · only read-only refusals reach the user (`frontend/src/main.tsx`); `CommentThread.tsx` clears a reply before the server answers; a failed anchor in `WorkspacePage.tsx::handleAddComment` closes the draft and drops its text.
+
+### g5.documents.comments.live
+
+- agent
+- Agent decision in the [comments design](../../chat_intent/comments-design/request.md).
+- Gap · only create and reply publish events (`comment_service.rs::create_comment`, `routes/comments.rs::add_reply`), both before the route commits; resolve, edit and delete publish nothing, and `frontend/src/lib/ws.ts` handles only `comment.new` and `comment.reply`.
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index 1ab30406..7c3d8eaa 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -25 +25 @@ Governing concepts: Review access follows [read authority](../concepts/security.
-Governing contracts: [review in a workspace you own](governance.g5.md#review-in-a-workspace-you-own), [publish existing work under the actor’s provider authority](governance.g5.md#publish-existing-work-under-the-actors-provider-authority).
+Governing contracts: [review in a workspace you own](governance.g5.md#review-in-a-workspace-you-own), [a review returns suggestions to the source owner](governance.g5.md#a-review-returns-suggestions-to-the-source-owner).
@@ -81 +81,11 @@ Governing concepts: Review access follows [read authority](../concepts/security.
-Governing contracts: [review in a workspace you own](governance.g5.md#review-in-a-workspace-you-own), [merge is a recoverable application of pinned intent](governance.g5.md#merge-is-a-recoverable-application-of-pinned-intent).
+Governing contracts: [review in a workspace you own](governance.g5.md#review-in-a-workspace-you-own), [a review returns suggestions to the source owner](governance.g5.md#a-review-returns-suggestions-to-the-source-owner).
+
+### Discuss a review suggestion
+
+[I · g5.story.discuss-a-review-suggestion]
+
+As a source owner, reviewer or reader, I can discuss a review suggestion in its own thread wherever the suggestion is shown.
+
+Governing concepts: Thread access follows [read authority](../concepts/security.g5.md#read-permission-is-explicit); messages are [durable platform records](../concepts/data_model.g5.md#the-api-owns-durable-platform-records).
+
+Governing contracts: [discuss a suggestion in its thread](governance.g5.md#discuss-a-suggestion-in-its-thread), [let every reader join a review thread](governance.g5.md#let-every-reader-join-a-review-thread), [show a suggestion the same way everywhere](governance.g5.md#show-a-suggestion-the-same-way-everywhere).
@@ -101 +111 @@ Scope: retained behavior with the P6 governance redesign still later work.
-An ordinary review composes checked fork, reviewer-owned child start, conversation and PR handback under existing publication and approval rules.
+An ordinary review composes checked fork, reviewer-owned child start, conversation and [suggestion hand-back](#a-review-returns-suggestions-to-the-source-owner) under existing publication and approval rules.
@@ -108,0 +119,42 @@ Explicit Finish stops the review child while dirty or unpushed work remains ther
+### A review returns suggestions to the source owner
+
+[D · g5.governance.review-suggestions]
+
+Push review records a round of suggestions on the source workspace — proposed changes, comments and artifacts — as durable platform records instead of a pull request. The source owner decides each suggestion and the reviewer may withdraw their own; recording a round moves no Git ref.
+
+This is the hand-back of an [ordinary review workspace](#review-in-a-workspace-you-own). The [scope exclusion](../scope.g5.md#ordinary-reviews-remain-ordinary-workspace-work) of sealed returned results, reviewer pipelines and result-triggered cleanup still stands.
+
+### Show a suggestion the same way everywhere
+
+[D · g5.governance.review-suggestions.display]
+
+A suggestion offers the same actions and the same thread in the Reviews panel and in a view of its file. Every suggestion selected for a file's view is reachable there: placed at its text when that is found uniquely, otherwise marked as unplaced — never hidden, and never presented as a match.
+
+Its controls follow the [permitted-action](documents.g5.md#offer-only-permitted-comment-actions) and [failure](documents.g5.md#keep-a-persons-words-when-a-comment-action-fails) rules for comments.
+
+### Discuss a suggestion in its thread
+
+[D · g5.governance.review-thread]
+
+Each suggestion has one thread: the reviewer's note opens it, every decision on the suggestion is recorded in it, and it carries over when a later round repeats the same proposal. The thread is kept with its suggestion and reached only through it; an [anchored-comment](documents.g5.md#the-current-owner-writes-comments) request never reads or changes it.
+
+### Let every reader join a review thread
+
+[D · g5.governance.review-thread.access]
+
+A suggestion's thread takes [replies from every reader](documents.g5.md#let-readers-reply-in-threads) of the source workspace, before or after the suggestion is decided. A reply carries no decision: accept, reject and resolve stay with the source owner, withdraw with the reviewer.
+
+### Notify the other party once
+
+[D · g5.governance.review-thread.notices]
+
+Each reply and each decision on a suggestion notifies the other party once: the reviewer of the owner's actions, the owner of the reviewer's, and both of another reader's reply. Nobody is notified of their own action, and a person holding both roles gets a single notice. A new round superseding earlier suggestions needs no notice per suggestion.
+
+Notices use [shared notification delivery](../software_architecture/platform.g5.md#shared-notification-delivery) and follow [notify after commit](../concepts/system_design_principles.g5.md#publish-notifications-after-authoritative-commit).
+
@@ -182,2 +234,2 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- From [v9](../../../docs/lean-backend-design-v9.md), [source lineage](../../uplift/g5-lean/expansion/stories.md#us52--return-a-review-change).
-- ~75% of retained composition · workspace_reviews.rs Finish/handback and workspace_review_service.rs; handback PRs and explicit Stop exist; full race/recovery verification is pending and redesigned P6 governance remains later work.
+- From [v9](../../../docs/lean-backend-design-v9.md), [source lineage](../../uplift/g5-lean/expansion/stories.md#us52--return-a-review-change). [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) replaced pull-request hand-back with Push review.
+- Partial · `routes/workspace_reviews.rs` Push review and `workspace_review_service.rs` return suggestions to the source; the pull-request hand-back and Finish paths no longer exist. Reassessed for thread and display paths only; round, race and recovery behavior is unexecuted.
@@ -212,2 +264,8 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- From [review-workspace lineage](../../uplift/g5-lean/investigations/product-operations.md#p04--review-workspaces-are-a-consequential-later-addition); [v9](../../../docs/lean-backend-design-v9.md) target-owner rule.
-- ~75% of retained composition · workspace_review_service.rs, workspace PR services; handback proposals exist; complete target-owner merge authorization and recovery remain unassessed.
+- From [review-workspace lineage](../../uplift/g5-lean/investigations/product-operations.md#p04--review-workspaces-are-a-consequential-later-addition); [v9](../../../docs/lean-backend-design-v9.md) target-owner rule. [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) made the choice per suggestion rather than a merge.
+- Partial · `routes/workspace_reviews.rs` lets the source owner accept, reject or resolve each suggestion; accept mechanics and recovery were not assessed for the comments design.
+
+### g5.story.discuss-a-review-suggestion
+
+- human-verified
+- The [comments design request](../../chat_intent/comments-design/request.md) names review-suggestion threads, shown on suggestion cards in the editors and the Reviews panel, as something that must work.
+- Partial · threads exist behind suggestion cards (`frontend/src/components/workspace/ReviewsPanel.tsx::RepliesThread`, `routes/workspace_reviews.rs`); the gaps are recorded under the review-thread contracts. Source-inspected, not executed.
@@ -224 +282 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- Agent design [P04](../../uplift/g5-lean/investigations/product-operations.md#p04--review-workspaces-are-a-consequential-later-addition); preserves [H081](../../uplift/g5-lean/candidates/human-decisions.md).
+- Agent design [P04](../../uplift/g5-lean/investigations/product-operations.md#p04--review-workspaces-are-a-consequential-later-addition); preserves [H081](../../uplift/g5-lean/candidates/human-decisions.md). The suggestion hand-back wording follows [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) for the [comments design](../../chat_intent/comments-design/request.md).
@@ -231 +289,37 @@ Apply the [failure and retry principle](../concepts/system_design_principles.g5.
-- ~85% · workspace_reviews finish; stop failure and retained-child UI paths unexecuted.
+- Gap · `routes/workspace_reviews.rs` has had no Finish operation since [PR #170](https://github.com/G5Labs/G5-Lean/pull/170); whether an explicit Finish is still intended was not re-decided by the comments design.
+
+### g5.governance.review-suggestions
+
+- agent
+- Recovered from merged [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) (kaihuiz-g5) and migration `021_review_suggestions.sql`; retained because the [comments design request](../../chat_intent/comments-design/request.md) asks for its threads to work. Its fit with [H081](../../uplift/g5-lean/candidates/human-decisions.md#h081--returned-result-reviewexplainer-product-workflow-removed-from-mvp) is a scope reading no human has confirmed.
+- Unassessed beyond threads · `backend/g5-api-core/src/services/workspace_review_service.rs`, `routes/workspace_reviews.rs`, the review handlers in `routes/internal.rs` and `ReviewsPanel.tsx` implement rounds, decisions and cards; only thread and display paths were inspected.
+
+### g5.governance.review-suggestions.display
+
+- agent
+- Agent decision in the [comments design](../../chat_intent/comments-design/request.md); [PR #173](https://github.com/G5Labs/G5-Lean/pull/173) introduced the in-file cards.
+- Gap · in-file cards in `frontend/src/components/workspace/ReviewSuggestionOverlay.tsx` never offer Withdraw. `SpecEditor.tsx` renders cards only inside rendered blocks, so one in a collapsed section, behind a tag filter or on a record's label-only first block disappears with no fallback list. Comment suggestions store a context record neither side reads, so one on a repeated line cannot be placed. In the read-only gutter of `CodeEditor.tsx` a suggestion marker replaces the comment marker and add-comment control.
+
+### g5.governance.review-thread
+
+- agent
+- Recovered from [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) (decisions recorded in the thread; threads carry over between rounds) and [PR #184](https://github.com/G5Labs/G5-Lean/pull/184) (the reviewer's note opens it). Reaching a thread only through its suggestion is an agent decision in the [comments design](../../chat_intent/comments-design/request.md).
+- Partial · `workspace_review_service.rs::create_suggestion_thread` and `add_thread_reply` store threads. `routes/comments.rs` and `comment_service.rs` apply no scope check, so the anchored-comment reply, resolve, delete and reply-list operations accept a suggestion thread, and a delete detaches it from its suggestion.
+
+### g5.governance.review-thread.access
+
+- agent
+- Recovered from [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) ("Any reader: … replies") and the route contract of `routes/workspace_reviews.rs::add_reply`; the shared reply rule now lives with comments after the human chose reader replies for anchored threads in the [comments design](../../chat_intent/comments-design/request.md). Replying after a decision is current behavior kept as an agent decision.
+- Implemented on the inspected path · `routes/workspace_reviews.rs::add_reply_as` admits any reader of the source, checks the source's mutation policy and leaves decisions to their own routes. Not executed for this design.
+
+### g5.governance.review-thread.notices
+
+- agent
+- [PR #170](https://github.com/G5Labs/G5-Lean/pull/170), [#173](https://github.com/G5Labs/G5-Lean/pull/173) and [#203](https://github.com/G5Labs/G5-Lean/pull/203) define reply and decision notices. Once-only delivery, excluding the actor and covering resolve and withdraw are agent decisions in the [comments design](../../chat_intent/comments-design/request.md).
+- Gap · `routes/workspace_reviews.rs` does not deduplicate reply recipients, notifies the reviewer of accept and reject even when the reviewer is the actor, and sends nothing for resolve or withdraw. `notification_service.rs` publishes each notice before the caller's transaction commits.
diff --git a/.g5/ontology/scope.g5.md b/.g5/ontology/scope.g5.md
index ba386db0..4d7b60e5 100644
--- a/.g5/ontology/scope.g5.md
+++ b/.g5/ontology/scope.g5.md
@@ -227 +227 @@ Current deferral follows the [erasure gate](#decide-erasure-policy-before-enabli
-- ~100% of sampled scope · task/fork/review routes; no dedicated result schema found.
+- Partial reassessment · task and fork routes show no sealed result schema. Review hand-back now stores rounds and suggestions (`review_publications`, `review_findings`, `review_suggestions`), which [governance](features/governance.g5.md#a-review-returns-suggestions-to-the-source-owner) treats as ordinary review hand-back; whether every part of that feature stays within this exclusion has not been reassessed.
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 90c8b797..61e90f6a 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -22 +22,2 @@ and [governance](../features/governance.g5.md); post-MVP governance retains its
-| CommentService | Anchored comment CRUD and replies | [Comments](#comment-anchors-and-replies) |
+| CommentService | Anchored comment CRUD and replies; thread storage for review | [Comments](#comment-anchors-and-replies) |
+| Review hand-back | Start, publish round, list, decide, withdraw, reply | [Review](#review-hand-back) |
@@ -61,2 +62,19 @@ parsing and authorized repository views.
-CommentService owns anchored comment and reply operations using the caller's selected
-repository version rather than inventing a separate content source.
+CommentService owns anchored comment and reply operations, including who may perform
+them, using the caller's selected repository version rather than inventing a separate
+content source. It is the only writer of thread and reply records and keeps
+[review threads](../features/governance.g5.md#discuss-a-suggestion-in-its-thread) out of
+reach of anchored-comment requests.
+
+### Review hand-back
+
+[D · g5.architecture.review_owner]
+
+The review owner records rounds and suggestions, decides who may act on a suggestion and
+its thread, and sends the parties' notices. It opens and extends suggestion threads only
+through [CommentService](#comment-anchors-and-replies) operations joined to its own
+transaction, so browser, agent-tool and internal entry paths reach the same checks.
+
+Applies [complete operations](boundaries.g5.md#keep-complete-operations-with-their-state-owner),
+[transaction composition](../concepts/system_design_principles.g5.md#compose-owners-without-splitting-atomic-effects)
+and [entry-path enforcement](../concepts/system_design_principles.g5.md#enforce-boundaries-on-every-entry-path);
+[governance](../features/governance.g5.md#a-review-returns-suggestions-to-the-source-owner) owns the behavior.
@@ -100,2 +118,8 @@ synchronization to API maintenance through shared typed Git readers.
-- [Design v9 §9.3](../../../docs/lean-backend-design-v9.md#93-editors).
-- ~60% · `backend/g5-api-core/src/services/comment_service.rs`; stored anchors and replies exist, but reanchoring lacks the specified hunk/proximity restriction and original-anchor rendering is not fully assessed.
+- [Design v9 §9.3](../../../docs/lean-backend-design-v9.md#93-editors); the only-writer sentence is an agent decision in the [comments design](../../chat_intent/comments-design/request.md).
+- Partial · `backend/g5-api-core/src/services/comment_service.rs` stores anchors and replies, but `workspace_review_service.rs` inserts thread and reply rows itself, the comment-id operations accept review threads, and `routes/comments.rs` rather than the service checks workspace ownership. Re-anchoring gaps are recorded with the [comment contracts](../features/documents.g5.md#reanchor-only-with-evidence-of-the-same-content).
+
+### g5.architecture.review_owner
+
+- agent
+- Agent decision in the [comments design](../../chat_intent/comments-design/request.md); [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) introduced the review service and routes.
+- Partial · `backend/g5-api-core/src/services/workspace_review_service.rs`, `backend/g5-api-routes/src/routes/workspace_reviews.rs` and the review handlers in `routes/internal.rs` own rounds, decisions and notices, but write thread rows with their own SQL. Only thread paths were inspected.
```

</details>

## Implementation decisions

Consequential choices made while implementing and reviewing, with the finding that prompted them where one did:

- **Assumptions resolved.** The ontology content read now returns the node file's `repoId`; Base refuses every thread write; `ScopedDb::on_commit` is the after-commit hook, and NotificationService uses it for every caller.
- **Mention edges kept under RLS.** Open question 2 stays open: the reversible option was implemented. Migrations 052 (columns), 053 (backfill) and 054 (NOT NULL, forced RLS), numbered after `main`'s 043–051 as it was merged, are split because the migration runner refuses populated-table data statements mixed with other DDL. The backfill uses the owner FORCE toggle it does accept. Rehearsed with rows as `g5_migrator`.
- **One writer, one gate.** CommentService checks a replier's read access itself (K10), and so does the review reply path for every entry. The shared thread writers refuse read-only workspaces and Base, so review decisions and replies follow the same rule (K21). Every decision, keeping an artifact included, also applies that check before any effect, because a suggestion whose thread row was deleted writes no note (round 2).
- **Failures stay on screen.** Resolve and delete wait for the server instead of updating optimistically, so a refusal shows on the thread that is still there, with any typed reply (K3). A failed refresh keeps the threads, cards and replies already loaded and says so beside them, instead of emptying the list (round 2).
- **Older contexts count only when they cannot have been cut.** New contexts always record whether each part was shortened. The earlier producer cut silently, and one cut removes at most 768 bytes, so an unflagged context over 3328 bytes gives no evidence (round 2).
- **Chosen lines stay chosen.** The read-only code viewer takes the selected or right-clicked row. A code draft captures its anchor when opened and is re-placed at submit, or kept open with the reason (K1, K8).
- **Cards in every view of a file.** Ontology views show the file's suggestion cards (K22). Cards and Accept finding read the source's writability (K5, round 2). Cards mark moved, lost and ambiguous placement in the document view as the code view does (K4). Twins that no published context singles out are marked ambiguous even at the published line (K20, reversed in round 2). The reviewer may withdraw even when also the owner (K6).
- **Legacy node threads stay reachable.** A directory node's view lists its node-level threads beside the graph, answerable and never startable there (K7, reversed in round 2).
- **Branch label.** An unlabelled comment is labelled with the workspace's branch (K13).
- **The page stays through a failed refresh.** What the workspace page shows stays as last loaded when refreshing it fails transiently (no answer, a timeout, rate limiting or a server error), and a status says so ([page refresh](ontology/features/workspaces.g5.md#keep-the-workspace-page-through-a-failed-refresh)). That covers its name resolution, detail, VM state, ontology tree, open file and approvals; the last four were added in round 6. Controls stay as last known and the server decides each action. A refusal (access lost, workspace gone) still shows the error. Approvals keep their fail-closed rule: their actions wait for current details and approvals, and a list that could not be refreshed is never shown as current. This replaced a store that kept drafts above the page (rounds 3–4), in which review rounds 4 and 5 kept finding defects: drafts crossing users, keys lost when blocks moved, duplicate and lost sends, lost errors. The human chose this fix in round 5.
- **One owner of what is typed, per page (round 10).** The workspace page holds every composer's state, keyed by what it is about:
  - the words, send in flight and error of a thread's reply box and of a suggestion's reply box;
  - a card's rejection reason, edited replacement and decision in flight or refused;
  - each file's new-thread draft, with its anchor.
  Composers read and write that owner, so removing or rebuilding one loses nothing: a switch between Source and Rich, a tab switch, regrouping, deletion, a commit, or a renderer switch. The owner also keeps, for a thread or card being answered, the record a view last showed of it, so a view built again can show it after it left the list (round 11). A send completes into the owner even if its composer is gone. The owner is scoped by workspace and by the principal it is rendered for, so another person or org starts empty. This decision supersedes "composers live as long as the editor" (round 5). It differs from the store removed in round 5, which lived above the page and keyed drafts by file alone. The human chose it in round 10, after rounds 6–10 kept finding views that unmounted composers.
  - What stays from earlier rounds, now for presentation only: both editors keep their comments panel mounted once shown, the Unresolved filter hides what it filters out (round 7), a review discussion is hidden rather than unmounted, and each suggestion card is rendered once into a node its place adopts (rounds 6, 7 and 9).
- **One principal per tab.** A tab acts for one person in one org ([`g5.security.login.principal`](ontology/concepts/security.g5.md#act-for-one-principal-per-browser-tab)).
  - `RequireAuth` keys what it guards by the person of the token in use. That principal is set the moment a token is adopted, before its profile loads.
  - An org switch keeps the page, so a flow that switched org goes on, and the composer owners start empty for the new org on their own. Keying the page by org as well reset the org-creation wizard as it switched to the new org; PR #271's browser run found it.
  - A periodic refresh or an org switch that adopts another principal drops the outgoing profile at once (the org switch in round 10).
  - A token refresh answered after another token was installed is dropped, a token or a refusal alike; the newer token stands. That holds for the provider's refreshes (round 14) and the API client's own refresh for a request answered 401 (round 15). A request answered 401 after its token was replaced starts no refresh (round 16).
  - The provider changes auth state only while the token it acts for is installed, checking again after each wait: an org switch, a periodic refusal and a silent adoption stop when another context arrived meanwhile, and sign-in runs once per mount rather than on every navigation, so nothing signs the tab back in after a sign-out (round 16). A renewal with the same person, org and scopes is not another context: a pending adoption or switch finishes with the renewed token (round 17).
  - A request answered 401 is sent again after the refresh only for the same principal.
  - A token rotation keeps everything (rounds 6 and 7; the round-4 store had cleared drafts itself).
- **No answer is not a refusal.** The API client reports any request without a usable answer as a network failure (status 0), including an answer cut off while read and a token refresh that could not connect. A token refresh answered 408, 429 or 5xx keeps the session too; only a refusal signs out. The periodic session refresh keeps the session after no answer (rounds 7 and 8).
- **Sent only for their principal.** Every comment- and review-thread write carries the principal the page or card was rendered for, and the API client refuses to send it under another principal's token. That covers starting, replying to and deciding a thread, and replying to or deciding a suggestion. It holds for a send still preparing its anchor, and for a click that arrives before the page is replaced. Scope changes for the same principal keep the page (round 8).
- **Deferred to a follow-up (round 8):** binding actions outside comment and review threads to the page's principal, an app-wide change.
- **One principal on screen.** A loaded profile is shown only while its token still names the principal in use, on every adoption path, so a late answer never shows one principal while requests go as another (round 9).
- **A composer keeps its subject in view.** While the owner holds words, a send, a decision, a reason or an edited replacement for a thread or a suggestion, the views keep it shown. The file view keeps it marked as deleted, decided or from an earlier commit, the directory view keeps its node threads, and the Reviews panel keeps its round where it was shown (rounds 9 and 10).
- **One card per suggestion in the Reviews panel.** Each card is rendered once and its place adopts it, as in the editors. A suggestion competing for the same lines is shown in the cluster, and its round points there; hidden rounds and collapsed groups keep their cards mounted (round 9).
- **A card being answered stays.** The file view asks for every status of the current commit and shows open cards, plus a decided card whose reply box holds unsent words or a send, with its decision, until those settle (round 8).
- **Deferred by the human (round 8):** splitting cached server data by principal, an app-wide change, is a follow-up task. The security block's metadata records the gap.
- **A draft being sent stays put.** While a new thread is sent, its draft cannot be closed or moved to another line or block, so a refused send is retried on its own anchor (rounds 5 and 6).
- **A decision is never undone by an older view.** A card kept from an earlier commit reads its status from every round on the file, and says so when they cannot be read (rounds 12 and 13). Every suggestion list is read through the latest decided view of each suggestion the client has read, from any list or decision answer, so a list answered after a decision, or one another view read, shows it too (round 14; round 13 rewrote the cached lists instead, which missed both). A kept record takes no view older than the one it holds: a decided status never gives way to an open one, and of two decisions the later stands. The server moves a status out of open only once, so an open view of a decided suggestion is always stale (round 13).
- **What can no longer be sent stays readable.** Where replying, deciding or starting a thread is no longer allowed (the workspace read-only, the ownership moved), the typed words stay readable and read-only, with Dismiss and no way to send them: replies and rejection reasons in round 12, edited replacements in rounds 10 and 12, and new-thread drafts in round 13.
- **Source lines as the file holds them.** The internal review-comment route records the confirmed line, and its context, with carriage returns and trailing spaces, as the editors split the file (round 6).
- **The code margin sits on the editor's rows.** The CodeEditor margin places each thread where the active editor draws its line: Monaco's own row positions (padding and suggestion cards included, read again when the text, its size or the cards change), or the read-only viewer's rows and cards. It scrolls with the text and reaches one screen past its end, so it never stops before the text does. The editable fallback wraps its lines, so its margin lists threads in line order instead (rounds 4–5; a pre-existing layout defect that blocked the requested outcome).
- **A chosen block stays chosen.** A document draft keeps the block chosen for as long as the text is unchanged since the draft opened, even beside identical paragraphs. After a change, the block is found again only where exactly one block matches; otherwise the draft stays open with the reason (round 5; this predated the branch).
- **Unknown is closed.** Thread controls on a review source need its loaded, writable state; while that state is loading or failed to load, none are offered, and a note says why (round 3). A transient refresh failure keeps the last-loaded state instead (round 5).
- **Placement needs evidence.** The internal review-comment route records the source head's line text when the agent sends none. A comment still without text, or an insertion no published context pins down, is marked unverified instead of matched (round 3).
- **A change is read after it happens.** A comment event, a reconnect, or a change committed here reads the affected lists again after it, cancelling a first read still out, which would otherwise answer with what it read before the change (rounds 15 and 16).
- **Merged with SpecEditor inline suggestions ([PR #265](https://github.com/G5Labs/G5-Lean/pull/265)).** `main` reads pending text changes inline in SpecEditor opened from PROJECT DOCS ([`g5.documents.suggestions.projection`](ontology/features/documents.g5.md#limit-suggestion-rendering-to-the-speceditor-entry-point)). Merged with this change:
  - only an open text change of the current commit is read inline; a decided card kept while someone answers it, and one kept from an earlier commit, keep their block card, with the decision or the earlier-commit mark;
  - a record's label-only title row shows its cards in the frame (`main`), and cards on collapsed or filtered blocks stay in the unplaced list (this change);
  - a comment started from a selection in the inline view sends the anchored draft (anchor built on the text shown, and the base OID), not `main`'s block id;
  - the display resolver keeps this change's rules for twins and insertions, on `main`'s linear line search;
  - this change's migrations followed `main`'s 046–048, as 049–051 at that merge.
- **Merged with remote-tracking display and automatic divergence completion (`main` `25f4ba15`).** `main`'s tracking pill, inline detail and unknown state read the sync-runs feed, while [the page refresh rule](ontology/features/workspaces.g5.md#keep-the-workspace-page-through-a-failed-refresh) keeps the workspace detail through a transient failure, so neither rule changes the other. `main`'s migrations 049–051 come first, and this change's follow as 052–054.
- **Merged with issue 238's selected-file checkpoints (`main` `72d2b4de`).** `main` publishes an explicit editor Save and an accepted review file through the selected-file checkpoint ([`g5.documents.save.checkpoint`](ontology/features/documents.g5.md#publish-an-explicit-editor-save), [`g5.governance.accept.checkpoint`](ontology/features/governance.g5.md#publish-accepted-review-files)). An acceptance whose publication fails stays accepted. That fits this change's rule that a decided suggestion never goes back to open: the Reviews panel card keeps the edit until the decision is sent, as before, and now shows `main`'s pending-publication notice. The HTML answer Apply stays save-only inside the preview's comment list.
- **Anchors hash without WebCrypto.** A page served over plain HTTP on a host other than localhost (the CI stack, or a self-hosted install without its TLS overlay) has no `crypto.subtle`. Anchor and content digests are then computed in script (`lib/sha256.ts`), with the same result, so comments can be sent there. PR #271's browser run found that no comment could be sent on such a page. `main`'s inline suggestions, which hash the shown text through the same function, are covered too.
- **Previews list threads beside them.** A rendered HTML preview without a source view lists the file's threads and cards beside it by source line, and starts none there (round 3).

## Requested outcome and scope

Make both thread kinds behave as the ontology above says. Anchored comments hold their line in every editor, can be read by everyone with access, are written by the current owner, and keep people's words when something fails. A review suggestion's thread works the same in the Reviews panel and in the file, cannot be reached around the review rules, and tells the right people once.

**Not in this change:**
- GitHub issue comments.
- The rest of the review product: rounds, accept mechanics, the current-commit rule, review copies, approval sync and the issue #238 auto-publish work.
- Mention matching and a mention picker.
- The unused cross-branch comment endpoint, `mark_branch_deleted` and the unused `CommentNewEvent`.
- List paging.
- Implementing hunk mapping.
- The stale `g5.governance.review-workspace.finish` body and a wider uplift of the review feature.

## High-level design

### Anchored comments

1. **One identity.**
   - Every editor surface lists by workspace, repository and path, and creates only line comments.
   - Ontology code and spec views pass the node file's repository and path, as a document tab already does.
   - The branch filter goes away (it stays accepted, but ignored, for compatibility).
   - Creating a new node-scoped comment is refused. Existing node rows are still listed with their node's view, marked unanchored.
2. **Anchor from what is displayed.**
   - SpecEditor and CodeEditor build the anchor from their own current text and hand the page a complete anchor.
   - The page no longer re-derives it from a loaded snapshot. It records the base OID the editor was showing, when there is one.
   - If the anchor cannot be built, the draft stays open with a message.
3. **One resolver, strict evidence.** `commentAnchors.ts` applies the order in `g5.documents.comments.reanchor`. Context is the two recorded lines on each side, and each side is compared as a unit. An empty or size-truncated side counts as no evidence. Examples:
   - 40 lines inserted above, paragraph and neighbours intact: both sides match and the line is unique in the file → it follows.
   - The paragraph above is edited: one side matches within 20 lines → it stays.
   - The commented heading is deleted, and an identical heading 61 lines away has different neighbours → orphan.
   - Two nearby equally good candidates → ambiguous.

   Line hashes are cached per text version, and the previous resolution stays on screen until the next one is ready, so typing stays smooth and threads do not flash "Orphaned anchor".
4. **Authority lives in CommentService.**
   - Its operations check the read-only state, read access for a reply and the current owner for everything else. Resolve, reopen and delete no longer require authorship; editing text still does.
   - Routes only translate requests and errors. The anchored reply route takes read scope, as the review reply route does, and a reader's reply still delivers its @mentions.
   - The editors offer "Add comment" to the owner whether or not the VM is attached. Every reader gets the reply box; Resolve, Reopen and Delete appear only for the owner.
   - Each comment mutation reports its failure in place and keeps the typed text. A submit in flight cannot be sent twice.
5. **Live changes.** Create, reply, update (resolve, reopen, edit) and delete each publish an event after commit, and the client refetches the affected lists on every one.
6. **Original anchors.** An orphan or ambiguous thread shows "was line N" and the anchored line text from its stored context.
7. **Small fixes in the same owners:**
   - Reject an empty message on create, as replies already do.
   - Show a collapsed thread's reply count.
   - Remove the mention-edge table (recommended; nothing reads it) or add tenant RLS to it, with a forward migration.

### Review-suggestion threads

1. **One writer, one gate.**
   - CommentService gains thread operations (open, reply, list) that the review owner calls inside its own transaction. The review owner keeps its authorization: any reader replies; decisions stay with their roles.
   - Anchored-comment operations refuse a `suggestion` thread id, so the generic reply, resolve, delete and reply-list operations no longer reach it.
2. **Attribution.** The [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md) removes agent replies and their attribution column; replies identify the person who wrote them.
3. **Notices.**
   - The review owner builds one recipient set per event: the other party, never the actor, deduplicated. Resolve and withdraw are added.
   - The shared notification producer publishes after the caller's transaction commits. This fixes the same defect for every notification caller, not only reviews.
4. **Display.**
   - In-file cards receive the viewer's role, so the reviewer can Withdraw there.
   - SpecEditor collects cards it cannot place in its rendered blocks (collapsed or filtered) into a visible "unplaced" list. A record's label-only title row shows its cards in the record's frame, as merged from `main`'s SpecEditor inline suggestions ([PR #265](https://github.com/G5Labs/G5-Lean/pull/265)).
   - The read-only CodeEditor gutter shows both the suggestion and the comment markers, and keeps the add-comment control.
   - Comment suggestions store the same context format the placement code reads, so a duplicate line can be told apart.
   - An arriving review notice also refreshes open threads.

### Rationale

- **Owner-only thread actions, open replies.** Starting and deciding threads stays with the owner, preserving the single-writer decision (H072). You chose to let readers reply, reusing the grant the review PRs gave review threads, so a mentioned teammate can answer.
- **One file identity.** It removes three per-tab code paths and makes it impossible for a label to strand a comment.
- **Two-tier re-anchoring.** It is the smallest precise reading of v9 that neither orphans a comment for an edit beside it, nor lets it jump to a distant duplicate.
- **One thread writer.** CommentService as the only writer gives both thread kinds one validation, attribution and event path, while their different grants stay explicit.

## Observable acceptance

1. With unsaved lines inserted above, and separately deleted above, a new comment attaches to the block the person chose. It is still there after Save and reload, and for a reader of the saved version.
2. The same file opened as a document, raw file and ontology view shows the same threads, and a comment added in any of them appears in the others. A legacy node comment remains visible, marked unanchored.
3. Re-anchoring behaves as in the four examples above; orphan and ambiguous threads show their original line and text.
4. A reader can reply in an anchored thread without a running VM, and a teammate mentioned there can answer. A reader sees no Add comment, Resolve, Reopen or Delete controls, and the API refuses those actions.
   - The owner can comment while stopped. After a transfer the new owner can resolve and delete the previous owner's threads.
   - A refused or failed create or reply shows why and keeps the text.
5. A second browser sees create, reply, resolve, reopen, edit and delete without reloading. An event subscriber that queries on receipt always finds the change.
6. Generic comment operations return not-found for a suggestion thread. A non-owner, non-reviewer reader can reply through the review route; decision routes refuse them.
7. Human replies identify their author consistently in the panel and the file card.
8. Each reply and decision produces exactly one notice for the other party and none for the actor, including when reviewer = owner. No notice precedes its commit.
9. The reviewer can Withdraw from an in-file card. A card whose block is collapsed or filtered appears in the unplaced list. Comment markers stay visible on lines with suggestions. Double Enter sends one reply.

**Verification:**
- API tests under the restricted runtime role, for allowed and forbidden paths.
- Frontend tests at the WorkspacePage–editor boundary, driving real editor text rather than stubbed props.
- The resolver's table of cases.
- One Playwright flow: comment, Save, reload, reader view. This is the case the v9 plan listed and never implemented.

Report inspection, test results and browser observation separately.

## Risks, assumptions and open questions

**Risks:**
- **Concurrent #238 work.** Draft PRs #254 and #257, and a local `feat/review-save-autopublish` branch, change review routes, both editors, the Reviews panel and `documents.g5.md`, and #254 adds migrations 043–044. Choose migration numbers and rebase at implementation time; none of that work touches thread or reply code.
- **More orphans for some edits.** A line becomes orphaned when the lines on both sides of it changed, even if it did not move, or when only one side still matches and it moved more than 20 lines. Today such a line follows whenever its text is unique in the file, which is also how a comment jumps to an unrelated twin. Hunk mapping, not added here, is the future remedy for large committed moves.
- **Node-scope creation refused.** Refusing new node-scoped comments changes the API. `api_comments.rs::test_create_node_comment_returns_201` and any end-to-end use of it must change in the same PR, and an old browser tab during rollout gets a validation error.
- **Wider notification change.** Publishing notifications after commit changes timing for every notification caller.

**Assumptions for source inspection during coding:**
- The ontology content read can return the node file's repository and path, including derived spec paths.
- Base refuses comment writes.
- Whether ScopedDb or the routes offer an after-commit hook, and what else calls NotificationService.
- How many legacy node rows exist.

**Open questions for the human (not blocking):**
1. Does H081's exclusion of a returned-result product still allow review suggestions? This design assumes yes because you asked for their threads to work.
2. Should the mention-edge table be dropped or kept under RLS? The recommendation is to drop it.
3. Should a reader's reply notify the thread's author? Today only @mentions notify. The recommendation is one notice to the author, never the replier; it is not part of this design.

The first question of the original design — whether readers may reply in anchored threads — was answered yes; see [the chat intent](chat_intent/comments-design/request.md).

## Design review

This was a self-review in the authoring session, applying the [design guide](../.agents/skills/ontology-authoring/references/design_review.md) to this file and the ontology diff. It found and repaired seven issues:

1. A "store it once" retry guarantee would have mandated a new replay mechanism for a single insert; it was dropped. A lost response can still yield a visible duplicate, which the owner can delete.
2. The rule separating review threads from anchored comments was stated twice; it now lives in governance and the module page links to it.
3. Comment authority sat in routes; the module block now makes CommentService decide it, per [adapters](ontology/concepts/system_design_principles.g5.md#keep-adapters-free-of-domain-policy).
4. Editor performance and the dirty-buffer rules were missing from the governing set; they were added.
5. The notice rule would have sent one notice per superseded suggestion; a round now needs none.
6. The display rule would have forbidden today's clearly labelled "lost" card at its published line; it now forbids only hiding a suggestion or presenting it as a match.
7. The reply-access rationale wrongly called a reply "the one" non-owner record; the round itself is another.

A second pass, taken from the security, lifecycle, workspace, notification and external-issue owners, confirmed two points:
- Comment writes are durable actions, needing no VM gate, and transfer carries authority.
- Honest attribution matches `g5.external-issues.publication.automation` and `platform_reports`.

It also left one point open: Base refusal is intended by `g5.workspace.identity.base` but not yet inspected in code.

**Amendment after the human's answer.** Reader replies became `g5.documents.comments.replies`, the single home of the reply grant for both thread kinds. Review access now links to it instead of restating it, and the owner rule no longer lists replies. A re-check covered four points:
- The grant confers no workspace-content write, so `g5.security.tenant.write` still holds.
- A revoked reader is refused on their next request; the 60-second membership-cache delay was retired on 2026-10-09 (G5Labs/G5-Lean#405).
- Controls now show readers the reply box only.
- Mentions in reader replies keep their existing workspace-access checks.
