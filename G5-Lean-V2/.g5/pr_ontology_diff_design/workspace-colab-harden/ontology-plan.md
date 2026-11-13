# One file editor across G5

The owner wants lower maintenance and the same experience wherever a file opens.
They have now specified **source-only Markdown editing**: rendered Markdown is for
reading. This supersedes the earlier Rich + Source recommendation. Implementation
has not started. [Human intent](../../chat_intent/workspace-colab-harden/unified-editor.md).

## User experience

- Files, Project Docs, Ontology and links resolve the same current file to one tab
  and session. Keep its draft, view and history; historical revisions stay distinct.
- Edit supported text files in Monaco. Markdown has Source and read-only Preview;
  HTML and JSONL retain their existing specialized previews, and other supported
  text files use Source. Opening defaults are
  still a UI choice; the decision establishes where editing happens.
- Share path/version, Save, suggestion, conflict and recovery controls. Keep actual
  permission, historical-source and stopped-compute boundaries.
- Preserve comments, questions, review decisions, ontology details and comparisons
  as domain features around the editor. Preview does not edit Markdown text.

## Ownership and reuse

One shared file editor coordinates navigation, the existing draft lifecycle and
source/preview views. Reuse the current Monaco wrapper as the only normal file-text
editing engine and its native undo/redo. Keep its model alive through preview
switches. Preview derives from the current source; it never writes rendered output
back into the draft. Retain loading-fallback drafts without a second editing engine.

Extend `fileDrafts` rather than create another registry. The session owns Monaco's
model, baseline and save/conflict coordination; retained draft snapshots support
recovery, not an independent editable buffer. Preserve person/workspace/repository/
path isolation and generation guards. Dirty/in-flight drafts survive view or tab
closure within the existing browser-tab lifetime. Clear the old person's drafts
on sign-out/person change; a late completion cannot mutate a replacement session.

Reuse existing document/file adapters and the shared backend Save operation. Save
captures submitted source and its baseline/VM/session, acknowledges only those
bytes, and retains later typing or failures. External changes cannot replace a dirty
buffer. Reader suggestions keep their existing authority and never directly save
source files. Keep domain metadata/cache updates in their existing owners.

Consolidate existing Markdown rendering into one file preview, retaining GFM,
math, Mermaid, relative links, source anchors and file-specific G5 features. Select
and extend an existing rendering pipeline against the feature corpus; source-only
editing removes the need for a rich-edit parser or cross-engine history. Classify
G5 features by the authorized file, not its opener; example fences do not become
live question actions. Preserve sanitization and HTML sandbox boundaries.

Remove SpecEditor's rich-input, structural text-editing and custom history code
after migrating required reading/review features. Ontology insertion/deletion also
moves to source. Keep Monaco DiffEditor and semantic comparison. Ordinary comment
and answer forms remain domain controls; chat rendering needs no separate rewrite.

## Verified starting point and corrections

Source inspection established existing reuse: SpecEditor/MarkdownRenderer share
parser helpers; ontology and Files already share some drafts; document and file
saves call the same backend helper. Project Docs does not currently join that draft
path, and navigation selects separate editors. See
[dispatch](../../../frontend/src/pages/WorkspacePage.tsx),
[drafts](../../../frontend/src/components/files/fileDrafts.ts) and
[Save](../../../backend/g5-api-routes/src/routes/workspaces.rs).

The existing 2-MiB guard is specific to FileView. No new universal limit is chosen;
resolve common source/preview capacity without silently reducing supported sizes.
No custom undo system, rich editor framework or application-wide chat rewrite is
required. Existing behavior evidence does not prove the new integration.

## Implementation and acceptance

First make one managed Markdown file open through Docs and Files into the same
session: edit source, preview, return, undo and Save. Then migrate other entry
points and remove the superseded paths. Verify exact submitted source, CRLF/Unicode,
IME, history, drafts, comments/questions, review placement and ontology controls.
Exercise actual Monaco plus navigation, failed/delayed Save, external edits, VM
replacement and authority/person changes. Run affected frontend checks and browser
journeys; do not infer runtime success from documentation checks.

Governing contracts: [common editor and modes](../../ontology/features/documents.g5.md#g5.documents.editor.experience),
[draft lifecycle](../../ontology/features/documents.g5.md#g5.documents.files-view.drafts),
[selected Save](../../ontology/features/documents.g5.md#g5.documents.selected-save),
[reader suggestions](../../ontology/features/documents.g5.md#g5.documents.reader-suggestions),
[review](../../ontology/features/documents.g5.md#g5.documents.review-placement) and
[ontology presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation).
No application code or runtime tests have been changed/run for the editor design above.

## Remove agent review comments — authorized implementation

The owner's [removal instruction](../../chat_intent/workspace-colab-harden/remove-agent-review-comments.md)
authorizes removing agent review-comment production and discussion, including the
internal findings/comment handoff, review-round read/decision tools, their MCP server
and launch credentials, and the Reviews action and prompt that invoke them. Retire
the installed G5 review MCP entry on the next launch without changing other user
configuration. No replacement agent comment channel is introduced.

The GitHub issue/PR comment commands and their shared publication/credential owners
remain unchanged. Formal agent approvals are a separate workflow and remain intact.
The editor design above remains design-only; this implementation does not start it.

Review publications, suggestions, findings and thread tables are shared with human
review; no table exclusively stores agent comments. The agent asked about broader
human-review removal, but no such extension has been authorized. The implementation
therefore removes agent data and support while preserving human reviews. Migration
113 deletes agent publications, suggestions, root/edit threads and replies across
tenants, drops the agent task, reply attribution and assessment payload columns,
and refuses agent authorship in the remaining review tables. Assessment suggestions
were produced only by the retired internal handoff; remove their kind, API field
and UI overlay as well. Historical migrations remain immutable
so existing installations can upgrade. Applied source-file changes and formal
approval records are outside this retirement; their independent history remains.

Validate removal through route/CLI refusal, launcher cleanup, affected browser
controls and a populated-database migration. Check retained human comments and
GitHub issue/PR comment commands at their existing interfaces.

Migration 113 uses the existing definer-role protocol for its data section and
restores the exact role membership and table privileges. Forced tenant RLS stays
in place. Its populated upgrade test passes for two organizations, including agent
threads inside human reviews, edit-thread references, human/general comments,
rejected agent authorship, dropped columns and a repeat no-op. The production
migration executable also passes the populated upgrade and repeat run.

Local validation passes: TypeScript compilation and affected frontend checks,
including 90 focused checks after the assessment cleanup; 58 human-review API
checks against the final schema; the four retired endpoint refusals; 33 human
comment API checks; and three GitHub issue/PR comment API checks. Unknown internal
paths are refused by authentication with 401 before routing, with no database
writes. The route/org/system authorization guards, launcher cleanup, retired-tool
admission, CLI parsing, provider launch and protected GitHub command regressions
also pass. Migration checksums and version uniqueness pass.

The final populated multi-tenant migration test and production executable's
upgrade/repeat test both pass. PR 803's first Cloud CI run found a stale retired
prop in an approval-page test helper, which the production-only TypeScript check
excluded. Removing it passes the full `tsc -b` check, the production bundle build
(with an 8-GiB Node heap), and all 29 approval-page tests; the migration test also
passes repository formatting. Cloud CI and reviews remain
pending. No deployment has been run; the schema cleanup takes effect when migration
113 is deployed.

PR review traced approval-thread ownership to the original and current writers:
human decisions create a fresh workspace thread with the decision's ID, while
formal agent decisions do not set a comment-thread link. Neither writer reuses
suggestion threads, so the proposed approval FK collision has no supported
execution path. The populated two-organization upgrade/repeat test now includes
human approvals and verifies the complete decisions and linked threads stay
unchanged; it passes under the same restricted migration role. Approval links
remain intact. Review also corrected the obsolete MCP documentation, the Files
guard citation and the existing editor design's file-type view wording.

The follow-up rebase onto main preserves the native issue tracker and its issue
comment API/CLI. Native issues now owns migration 112, so the unmerged review
cleanup and its populated-upgrade test move to 113; the SQL bytes are unchanged.
The migration catalog, checksums, Rust formatting and full TypeScript check pass.
The previous head's E2E failure was a first-attempt OAuth-fixture 401 in the
workspace review journey: the retry passed, but the mandatory receipt rejects
flaky runs. The rebased head requires a fresh clean CI run.

## One comment list beside Source and Preview — authorized implementation

The owner's [instruction](../../chat_intent/workspace-colab-harden/comments-side-list-in-source.md)
(2026-10-30) makes Source show its comments the way Preview now does: one side list
stacked from the top in the text's order, and an expanded card brings its line to the
top of the view. This amends [comments identity](../../ontology/features/documents.g5.md#g5.documents.comments.identity)
and the anchored-placement sentence of [review alternatives](../../ontology/features/documents.g5.md#g5.documents.review-alternatives).

Owners and reuse: the thread model owns the order (`orderBesideView`, shared by the
Source margin, the Markdown and HTML previews and node views), the comment column is a
plain list that reports toggles, and each view owns how it shows a line: Preview scrolls
the rendered block holding it, Source reveals the row at the top of the editor (Monaco)
or scrolls the read-only viewer to it; the editable fallback wraps lines and does not
scroll. Removed with the anchored margin: row geometry read from Monaco, the margin's
scroll synchronization and settlement rules, its extent spacer, overlap avoidance with
measured card heights, and the never-wired hover Add control. Retained: Source's
selection commenting, glyph anchor marks, the draft composer (first in the list, anchored
to the chosen line as before), draft and reply retention, and the distinct labels of
ambiguous, orphaned and unanchored threads.

Acceptance: in Source, threads on L20 and L150 list in that order whatever the text
shows, and expanding a card puts its row first in the viewport; in Preview the same list
scrolls the block to the top. Unit tests cover the shared order and the column; editor
tests cover the Monaco reveal and the read-only viewer's scroll; the browser flow
`comments.spec.ts` covers the Source list and reveal, and the Preview order and label
jump. The anchored-margin tests and their settlement traces are retired with the behavior.

Rebased over PR #967's one-surface switch (2026-10-30): the mode tabs are gone, so
"Preview" here is the rendered document and "Source" is plain text. The card label
reads "Open line N in plain text", and the page and browser regressions read the mode
switch (`file-view-switch-mode`) rather than the retired tab ids. Owners after the
rebase: `commentThreadModel.ts` the order, `CommentColumn.tsx` the list,
`DocumentPreview.tsx` the block scroll by source range, `CodeEditor.tsx` the plain-text
reveal and `FileView.tsx` the label's switch; unit, editor, page and browser
(`comments.spec.ts`) regressions cover order, reveal and jump. Rich text open for
editing lists the cards without scrolling, a gap left for a later decision.

## Select to comment in the rendered document — authorized implementation

The owner's [decisions of 2026-10-30](../../chat_intent/workspace-colab-harden/select-to-comment-rendered.md)
add a Comment bubble on selected text while reading the rendered document, write the new
thread's draft in the Comments panel with the page staying as it is, keep the bubble to
reading (not Rich text editing), gate it exactly as plain text's bubble, and show the panel
even when the file has no threads. This amends
[comments identity](../../ontology/features/documents.g5.md#g5.documents.comments.identity):
the rendered document now offers selection commenting, and the record Comment action writes
the same draft in the panel (agent choice, so the two ways to start a thread from the
rendered document behave alike).

Owners and reuse: one draft per file, under the page's composer subject plain text already
used (`commentDraft.ts`: open, cancel and send with anchoring, re-placement when the text
changed, retry identity), so a draft started in either mode is the same draft in the other
and survives tab switches ([drafts](../../ontology/features/documents.g5.md#g5.documents.comments.drafts)).
The card showing it (`CommentDraftCard`) and the bubble (`SelectionCommentBubble`) are shared
by plain text's margin and the rendered document's list; plain text's bubble logic is
unchanged. The rendered document maps a selection to a source line by its block's source
range and the first block line showing the selection's opening words
(`documentSource.ts`), so a selection across wrapped source lines or through inline markup
lands on its first line; when the rendering hides the source (a link's text), the block's
first line. Anchors stay line-based; the quote is the selection, as plain text quotes it.
The side list (`SideThreads`) takes whether Comments is on, hosts the draft first and shows
an empty state with a hint when a thread can be started. `FileView` hosts the rendered
document's draft and the gate; `CodeEditor` keeps hosting plain text's. The retired
`previewCommentTarget` plumbing (record Comment opening plain text) is removed.

Not done, by decision: no bubble while Rich text is open for editing; no inline highlights
of commented text. Known approximation: a selection whose opening word repeats earlier in
the same block anchors to that earlier line; the quote keeps the thread readable.

Acceptance: while reading, selecting words shows the bubble; its click opens the draft first
in the panel with the selection quoted, the page unmoved; sending anchors the thread to the
selection's first line and lists it; the bubble is absent for a reader of unpushed content,
while Rich text is being edited, and for a selection inside a card or control; the Comments
control shows the panel with an empty state. Unit tests cover the line mapping, the list's
empty state and draft card, the bubble in the rendered document and both FileView entry
points; the page test covers a reader on pushed content; the browser flow selects text in a
rendered file, saves the thread and sees it listed, and opens an empty file's panel.
