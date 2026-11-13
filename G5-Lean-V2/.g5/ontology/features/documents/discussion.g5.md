# Documents: review and discussion

Review display, comments and discussion, and project questions. See [Documents](../documents.g5.md#g5.documents).

## Review display

<a id="g5.documents.review-placement"></a>
[D · g5.documents.review-placement]

The Markdown Preview places pending edits at verified source locations, including separators and file boundaries, with their decision cards in the [margin](#g5.documents.comments.identity) level with the edited text. Display format never invalidates source evidence or changes proposal bytes.

- When rendered text hides changed whitespace, show a local source diff with visible whitespace; Source view uses Git diff presentation.
- Select these displays by the current file view, whatever the entry point.
- Relocation must agree with [acceptance](../workspaces.g5.md#g5.workspace.review-acceptance); missing or ambiguous evidence shows original, current and suggested source in a conflict view.
- Any future manual location choice requires a fresh server source check first.
- Reuse the same [decisions and discussion](../workspaces.g5.md#g5.workspace.review-decisions) in each display.

<a id="g5.documents.review-alternatives"></a>
[D · g5.documents.review-alternatives]

For any number of proposals sharing a verified document location, Preview shows one in-document preview and keeps every independent decision card in the margin. Each connected set of conflicting edits is one margin box that names how many proposals conflict and which is previewed, holds each alternative's card and decisions, and previews the chosen alternative; compatible edits keep separate cards.

- Select one alternative per conflict set, composing compatible edits and selected alternatives from disjoint conflict sets.
- Overlapping or transitively conflicting proposals never compose into a new suggested result.
- Edits whose non-empty source ranges only touch, sharing a boundary but no byte, do not overlap: composing them applies both, so removing both copies of a doubled word leaves neither, exactly as acceptance would write it.
- Selecting a card changes presentation only, preserving unchanged context, one canonical block and comment identity, and each edit's [drafts](#g5.documents.comments.drafts).
- Comments keep canonical source anchors when selection changes, and both modes use the same shared [discussion list](#g5.documents.comments.identity).
- Missing or ambiguous placement follows [review placement](#g5.documents.review-placement); a source refresh rechecks remaining proposals under the existing [acceptance](../workspaces.g5.md#g5.workspace.review-acceptance) owner.
- Each alternative's card in a conflict box carries the preview choice, checked on the previewed one; Preview omits the original-parent-context disclosure but keeps the failed edit and current source for conflicts.
- Provisional, pending the owner's confirmation: the box presentation and separate cards for compatible edits.

## Discussion

<a id="g5.documents.comments"></a>
[D · g5.documents.comments]

People discuss a workspace in threads — general discussion, or comments anchored to file text — shown beside the document and reachable from a workspace-level list.

<a id="g5.documents.comments.identity"></a>
[D · g5.documents.comments.identity]

File comments retain their workspace, repository/path, displayed-file and line evidence; branch names are labels. General workspace discussion uses the same canonical comment and reply records without a file anchor, and the workspace timeline projects those threads to their original context.

- Reposition a thread only when retained evidence identifies one location; missing or ambiguous placement stays at its original context.
- Every thread stays reachable from a workspace-level list, including one whose file was renamed or deleted, shown with its retained context; discussion leaves view only when its thread is deleted. Review suggestion threads keep their separate review owner.
- A file's comments and replies appear beside the text in both [modes](editor.g5.md#g5.documents.editor.modes), in text order: placed threads by line then creation time, and unplaceable threads after them in creation order.
- In the rendered document they share one margin with the suggestion cards, each level with the block holding its line, later cards pushed down, and the last expanded, selected or inline-reached card set level with its text again.
- The margin shows suggestion cards whenever the file has open ones (provisional, pending the owner's confirmation) and comment cards while Comments is on; plain text keeps a top-stacked side list, where expanding a placed card brings its source line to the top.
- Where [comment authority](#g5.documents.comments.authority) allows, selecting text while reading the rendered document offers Comment as plain text does: the thread quotes the selection and anchors to its first source line, or its block's first line when the text cannot be located.
- A new thread from the rendered document is written in the margin level with its line, leaving the page in place; a placed card's line label opens plain text at that line. One new-thread [draft](#g5.documents.comments.drafts) spans the file's modes and tab switches.
- Before and deleted comparison rows, verification details, findings controls and All Changes offer no commenting, and records have no separate Comment action.
- Plain text keeps anchor highlights; Rich text open for editing offers no selection comment, and the rendered document has no inline comment highlights or Unresolved filter. The Comments control shows the margin or side list even with no threads.
- Retiring a discussion identity requires verified absence or an explicit preservation decision; a failed absence guard leaves the schema and history intact.

<a id="g5.documents.comments.authority"></a>
[D · g5.documents.comments.authority]

Eligible current readers may start general workspace discussion or threads anchored to authorized pushed file content, and may reply while compute is stopped. A thread is discussion, not a content write, so read-scoped credentials start threads, reply and edit their own text, while deciding a thread requires write scope and ownership.

- Only the owner anchors current unpushed work, resolves, reopens or deletes threads; only the original author edits their own text.
- Base, read-only and freezing content policy still refuse these writes.
- Validate pushed anchors against authorized Git bytes and publication evidence, and hold current read access until commit; author text edits require the same hold.
- Publish canonical discussion events only after commit; native file, document, diff and timeline views reuse the same records and refresh after a mutation or reconnect.

<a id="g5.documents.comments.drafts"></a>
[D · g5.documents.comments.drafts]

Draft text, pending sends and failures belong to the person and the page subject, including replies, rejection reasons, edited replacements and new-thread drafts. View changes, source commits, regrouping or removal of the displayed target must not discard them.

- Loss of authority leaves text readable and dismissible without offering a write.
- A person or organization change isolates the old store, including late completions.
- A failed action retains the text and shows the failure.
- Retained cards accept newer decisions; an older cached open record cannot reopen them.

<a id="g5.documents.intent-history"></a>
[D · g5.documents.intent-history]

Traceability records distinguish stored human messages from inferred links and current extracted source. Read history within the authorized workspace/repository cut, retain the original chat identity and text, and expose stale or malformed sources without inventing recovered intent.

## Project questions

<a id="g5.documents.question-decisions"></a>
[D · g5.documents.question-decisions]

Project question sets distinguish human decisions from agent proposals. Only the author may change or clear a recorded human decision, while a person may adopt or clear an agent proposal.

- Answer commands preserve question identity and wording.
- A request matching the question's retained receipt is an idempotent retry; a different request reusing that receipt's key is refused, and other commands require the current question revision.
- A person's answer or set creation uses [selected-file Save](files.g5.md#g5.documents.selected-save); agent proposals stay live until published, and failed publication is retried without answering twice.

<a id="g5.documents.question-votes"></a>
[D · g5.documents.question-votes]

Workspace readers other than [organization viewers](../organizations.g5.md#g5.organization.viewer-collaboration-question) may cast, replace or withdraw their own advisory vote on a referenced project question without starting compute.

- A vote identifies the signed-in voter and resolved question revision, and selects only offered choices.
- Votes neither become recorded answers nor change the question, and do not follow Git fork inheritance.
- [Meeting decision cards](workbench.g5.md#g5.workbench.meeting) show no vote controls.

## Metadata

### g5.documents.review-placement

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: [issue #574](https://github.com/G5Labs/G5-Lean/issues/574), the earlier [PROJECT DOCS scope](../../../chat_intent/main/speceditor-suggestions.md), and [implementation design](../../../pr_ontology_diff_design/fix-suggestions-render/ontology-plan.md).
- The [unification request](../../../chat_intent/workspace-colab-harden/unified-editor.md) and [design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md) replace opener-specific display with Preview/Source display. Existing Project Docs projection evidence does not establish all-entry-point integration.

### g5.documents.review-alternatives

- human-verified
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: the human's 2026-10-15 SpecEditor overlapping-suggestions request, approved document-left/cards-right mock, implementation authorization, and follow-up requiring arbitrary author counts and removal of “Previewing in document” and “Original proposal context”.
- Implementation: `specReviewProjection.ts` owns verified display grouping; `SpecReviewSuggestions.tsx` selects one alternative and retains independent `SuggestionCard` instances. `DocumentPreview.tsx` retains the canonical source and shared side discussion list when selection changes. Projection, real-renderer and browser regressions exercise these boundaries; backend decision ownership is unchanged.
- The [unification design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md) retains these rules in shared Preview; existing evidence does not verify their integration from all file entry points.
- Source’s anchored comment placement was replaced by the shared list on 2026-10-30 at the owner’s [instruction](../../../chat_intent/workspace-colab-harden/comments-side-list-in-source.md): plain text reveals an expanded card’s line instead of placing the card beside it ([design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#one-comment-list-beside-source-and-preview--authorized-implementation)).
- The owner chose the aligned margin for the rendered document on 2026-10-31 ([instruction](../../../chat_intent/kaihui-suggestion-mode-improvement/cards-out-of-document.md), question set `suggestion-cards-beside-document` in Project Docs): the cards left the in-document grid, one box per conflict set, compatible edits as separate cards; [design](../../../pr_ontology_diff_design/kaihui-suggestion-mode-improvement/ontology-plan.md). The box style, separate cards and the margin showing with Comments off are agent proposals awaiting the owner’s confirmation. `documentMarginLayout.ts` owns placement, `DocumentMargin.tsx` measurement, `SpecReviewSuggestions.tsx` the cards through margin portals.
- Amendment: [supplied proposal](../../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 9 (DC-Q3).

### g5.documents.comments

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.comments` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Identity](#g5.documents.comments.identity), [Authority](#g5.documents.comments.authority), [Drafts](#g5.documents.comments.drafts).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.comments.identity

- agent
- Expands: [Comments](#g5.documents.comments).
- [Owner review of PR #798](https://github.com/G5Labs/G5-Lean/pull/798#issuecomment-6393890252), item 6, requests explicit documentation of Preview’s side-list and Source-jump decision. The shared-editor design preserves discussion records and authority, not retired rich-editor selection or comment geometry.
- Owner decisions of 2026-10-30, after PR #969 ([instruction](../../../chat_intent/workspace-colab-harden/select-to-comment-rendered.md)): a Comment bubble on selected text while reading the rendered document, the draft written in the Comments panel, the bubble offered exactly when plain text’s is, and the panel shown when the file has no threads; design in the [workspace plan](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#select-to-comment-in-the-rendered-document--authorized-implementation). Agent choice there: a record’s Comment action writes the same draft in the panel instead of opening plain text.
- [Issue #964](https://github.com/G5Labs/G5-Lean/issues/964) (2026-10-30) ordered the side list by placed line, then creation time, with ambiguous, orphaned and unanchored threads after them. The owner then asked that an expanded card bring its line to the top, not the center, and that plain text behave the same ([instruction](../../../chat_intent/workspace-colab-harden/comments-side-list-in-source.md)), which replaced the anchored Source margin with the shared list; owners, removed mechanisms and test layers are in the [design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#one-comment-list-beside-source-and-preview--authorized-implementation). On 2026-10-31 the owner chose a margin aligned to the text for the rendered document, comments and suggestion cards together ([instruction](../../../chat_intent/kaihui-suggestion-mode-improvement/cards-out-of-document.md), [design](../../../pr_ontology_diff_design/kaihui-suggestion-mode-improvement/ontology-plan.md)); the stacked list and reveal remain plain text’s, and the list of Rich text open for editing is unchanged. After the [one-surface switch](editor.g5.md#g5.documents.editor.modes) (PR #967) the two modes are the rendered document and plain text. Gap: while Rich text is open for editing, expanding a card does not scroll the editor; the rendered reading view and plain text do.
- Source: human [comments request](../../../chat_intent/comments-design/request.md) and [workspace reconstruction](../../../uplift/ben-ontology/investigations/workspace.md). Amended by [issue #620, PR B step 5](https://github.com/G5Labs/G5-Lean/issues/620): the 2026-10-17 inventory establishes no node threads on the Lean forward-migration path; migration 090 enforces that prerequisite without converting or deleting history. Production requires the separate migration-039 reset. PostgreSQL 17 rehearsals through the production runner as `g5_migrator` passed populated success, node-thread/reply refusal, fresh installation and no-op rerun; `migration_090_retire_projections` checks retained rows, privilege restoration and forced RLS. Actual file-thread/reply/moderation API regressions pass with no ontology rows.
- [CommentPlacement](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked that finite text/context evidence yields a unique placement or visible ambiguity/orphaning. Capture, hashing, parsing, distant fallback and semantic identity beyond retained evidence are unmodeled; no particular placement algorithm is mandated.
- Amendment: [supplied proposal](../../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 10 (DC-Q4).
- Amended by [issue #925, G1–G3](../../../chat_intent/issue-925/decisions.md#human-scope--2026-10-28) (owner decisions of 2026-10-28): selection commenting in the rendered reading view as well as in plain text, and no record Comment action. Rich text editing has no commenting (agent choice, [plan](../../../pr_ontology_diff_design/issue-925/ontology-plan.md#commenting)).
- Rebased onto main on 2026-11-03: #925's selection commenting uses main's implementation from the owner's 2026-10-30 and 2026-10-31 decisions (`commentDraft.ts`, `SelectionCommentBubble`, the margin beside the rendered document). On top of it #925 keeps Monaco's own selection and Add Comment action in plain text (G1a, `CodeEditor`), no commenting in Before and deleted rows or on All Changes, the reader gate after Done ([authority](#g5.documents.comments.authority), R11, `FileView`) and no record Comment action (G3), which main's text still mentioned. Tests: `DocumentPreview.test.tsx`, `FileView.test.tsx` and `e2e/mock-preview/file-editor-925.spec.ts`.

### g5.documents.comments.authority

- human-verified
- Expands: [Comments](#g5.documents.comments).
- Source and checks: [workspace reconstruction](../../../uplift/ben-ontology/investigations/workspace.md). Reader-started threads: the human's [request](../../../chat_intent/comments-readers-start-threads/request.md) (2026-10-15) replaced owner-only thread creation; design in [comments-readers-start-threads](../../../pr_ontology_diff_design/comments-readers-start-threads/ontology-plan.md).
- CommentAuthority checked owner thread actions, author edits and reader replies before the amendment; reader-started threads are covered by tests, not by that model. Notification separately checked publication after commit. These finite guards do not prove the complete RLS schema or comment placement.
- Implemented · `comment_service.rs::create_comment`, `add_reply` and the content branch of `update_comment` check the author can read the workspace and hold that access until commit (`scoped.rs::ensure_user_workspace_access_until_commit`: active organization membership and workspace identity share-locked, since tenant row security admits any org member and [current authority](../../concepts/security.g5.md#g5.security.current-authority) must survive to commit); resolve, reopen and `delete_comment` keep `ensure_workspace_owner`. `routes/comments.rs` registers `create_comment`, `add_reply` and `update_comment` with read scope; `update_comment` refuses a `resolved` change unless the credential itself carries write scope, then opens the owner-only write-scoped transaction. `WorkspacePage.tsx` derives discussion and reply admission from current read access and moderation from ownership; current unpushed anchors retain their owner-only boundary. API tests (reader starts and edits, owner and write-scoped non-owner decide or not, the owner's read-only token edits but cannot decide, personal privacy refusal, revocation waits for an open write, former owner), the page test for a reader and the browser flow `frontend/e2e/flows/comments.spec.ts` (a reader starts a thread the owner sees live and resolves).
- The [issue447 human direction](../../../chat_intent/qa-review-prototype/request.md) adds general workspace discussion and pushed-anchor validation. Native-reader retry and owner/reviewer diff-to-timeline coverage remains governed by [the existing review design](../../../../docs/review-flow.md).

### g5.documents.comments.drafts

- agent
- Expands: [Comments](#g5.documents.comments).
- Source: human [comments request](../../../chat_intent/comments-design/request.md), rounds 11–14, and page-owned ComposerStore/captured-store callbacks.
- [DiscussionDrafts](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked retained text after view, target or authority loss, late completion against its captured store and refusal of stale open records. The scope is a mounted page/session; browser restart, every renderer and arbitrary new typing during a successful send are excluded.

### g5.documents.intent-history

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: [workspace reconstruction](../../../uplift/ben-ontology/investigations/workspace.md), `routes/grounding` lines 1–190,408–585,664–780,1227–1405 and `routes/intent_log` lines 1–70. Captured reads use the [reference owner’s evidence](files.g5.md#g5.documents.captured-references); extraction diagnostics and parsing were inspected, not modeled.

### g5.documents.question-decisions

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source and checks: [chat/identity reconstruction](../../../uplift/ben-ontology/investigations/chat-identity.md).
- QuestionDecision distinguished human answers from agent proposals; QuestionOwnership checked author protection, adoption/clearing and unchanged question shape. QuestionReceipt checked the current retained receipt, conflicting replay, revision checks and valid A→B→A commands, without a lifetime key ledger.
- QuestionPublication checked one-receipt retry without reanswering and publication of the expected version, conditional on [selected Save](files.g5.md#g5.documents.selected-save) capturing those bytes. Its blob/native-writer gaps prevent an implemented end-to-end guarantee; attribution proves neither content integrity nor formal approval, and separate authorized wording edits remain valid.

### g5.documents.question-votes

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source and checks: [chat/identity reconstruction](../../../uplift/ben-ontology/investigations/chat-identity.md), vote route/service bindings.
- QuestionVotes checked own-vote replacement/withdrawal, offered choices, revision attribution and no change to the recorded answer. Two voters and two revisions are modeled; revision resolution is not an atomic latest-file lock.
- Authority hold: [issue #660](https://github.com/G5Labs/G5-Lean/issues/660). `QuestionVoteService` holds the voter's workspace access until the vote or withdrawal commits (`g5.security.current-authority`).
- Amendment: [supplied proposal](../../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 11 (OR-cell-16).
