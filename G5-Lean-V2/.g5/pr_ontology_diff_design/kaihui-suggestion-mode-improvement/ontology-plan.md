# Suggestion cards in a margin beside the rendered document

Source: the owner's [request and decision](../../chat_intent/kaihui-suggestion-mode-improvement/cards-out-of-document.md)
(2026-10-30/31), the Project Docs page `suggestion-cards-beside-document.md` with its
mock and question set `suggestion-cards-beside-document` (docs repository, workspace
`88ef1efd-b5e0-4a93-a380-1ac2dce5722a`).

## Decision and assumptions

The owner chose **A2: a margin aligned to the text**, as mocked: suggestion cards and
comment cards share one margin to the right of the rendered document, each level with
its text, later cards pushed down to make room, and the card last selected or expanded
set level with its text. The document keeps its full width and its inline marks.

Assumed until the owner changes the proposals recorded on the question set:

- **B1** one box per connected set of conflicting proposals, the alternatives stacked
  inside it, each with its own card and decisions; a radio previews one in the document.
- **C1** independent edits on one paragraph keep separate cards, so a shared box always
  means "only one of these can apply".
- The margin shows suggestion cards whenever the file has open suggestions, with Comments
  on or off; the Comments control shows and hides the comment cards and the draft.
- Comments move into the margin too, as the chosen mock shows. This reverses, for the
  rendered document only, the 2026-10-30 instruction that put comments in one stacked
  list with an expanded card scrolling its block to the top. Plain text keeps that list
  and reveal; Rich text open for editing and HTML previews keep the stacked side list.

## Amended commitments

- [review placement](../../ontology/features/documents.g5.md#g5.documents.review-placement):
  edits stay in the document at their verified locations; their cards are in the margin
  level with them.
- [review alternatives](../../ontology/features/documents.g5.md#g5.documents.review-alternatives):
  the cards are beside the document, not beside each preview; one box per conflict set.
- [comments identity](../../ontology/features/documents.g5.md#g5.documents.comments.identity):
  the rendered document's margin replaces its stacked list; plain text unchanged.

## Owners and reuse

- **Placement** (`documentMarginLayout.ts`): a pure function from each item's anchor top
  and height, plus the active item, to item tops. Forward pass places each item at its
  anchor or below the previous item; the active item is set at its anchor and the items
  before it pushed up (never above the margin's top) and the items after it pushed down.
  Items without a place follow the placed ones, in the order given. Unit-tested alone.
- **Margin** (`DocumentMargin.tsx`): owns measurement only. It reads anchor positions
  from the document (`[data-block-index]`, the review preview's marks) relative to the
  row holding the document column and the margin, watches sizes with `ResizeObserver`,
  and lays items out with the function above. It renders the items it is given, keyed
  by id, so a card keeps its component (typed words, a send in flight, its error) when
  it moves (`g5.documents.comments.failure`); nothing of the cards' state lives here.
- **Suggestion cards** keep their owner, `SpecReviewSuggestions`: it still owns a group's
  selection and alternative preview, renders the preview rows in the document and now
  renders the cards through portals into margin nodes the host supplies, the mechanism
  `SuggestionCardPlaces.tsx` already uses for plain text's zones. A conflict set renders
  one `ConflictBox` holding each alternative's unchanged `SuggestionCard`; the radio
  previewing an alternative replaces the "Preview in document" button. Compatible edits
  are separate items anchored to their first mark.
- **Notes** (non-hunk and decided suggestions, `BlockSuggestions`) become margin items
  anchored to their block; nothing of them stays in the document tree.
- **Comments**: `DocumentPreview` renders the draft and the threads as margin items
  anchored to the block holding their line, with the thread model's order on ties and
  one expanded thread at a time. `FileView` passes the comment props it gave
  `SideThreads` to `DocumentPreview` for the rendered reading view; `SideThreads` stays
  the list for Rich text editing, HTML previews and the deferred preview.
- **Anchors**: a thread's block by `renderedBlockForLine`; a suggestion's first mark by
  `data-review-anchor`, else the group's container; a filtered group's placeholder; none
  for attention cards and unplaced threads.

Removed: the in-document grid (`spec-review-layout`), the `g5-note` tree nodes and the
block wrapping they needed, the review-reveal scroll on expanding a comment card in the
rendered document (the card is beside its text; `revealLine` stays for other callers).

## Acceptance and tests

- Opening a file with suggestions on two paragraphs shows the document at full width
  with its marks; each card sits level with its paragraph; the conflict box names the
  previewed reviewer and previewing another changes the paragraph; accept, edit, reject,
  withdraw and discussion keep working from the margin with unchanged requests.
- Clicking a mark sets its card level with the mark; expanding a comment card sets it
  level with its block, and a card pushed down by its neighbours returns beside its text
  when selected.
- With Comments off and open suggestions, the margin shows only suggestion cards.
- Unit: the layout function; `DocumentReviewPreview.test.tsx` (cards in the margin, the
  radio, decision identity and drafts unchanged); `DocumentPreview.test.tsx` (comments in
  the margin, order, empty state, Comments off); `SideThreads` tests unchanged. Browser
  flows that pressed "Preview in document" press the radio instead; they are updated but
  not run in this VM (no stack), reported as a gap.

## Implementation (2026-10-31)

Built as designed. `documentMarginLayout.ts` (placement, unit-tested), `DocumentMargin.tsx`
(measurement, `ResizeObserver`; `useRowStacked` tells the host when the row is narrower
than 768px, and the host then renders the margin under the document row, where an
expanded thread scrolls its block to the top as the list did), `SpecReviewSuggestions.tsx` (preview rows in the document; each card and each
conflict box heading rendered through a portal into a margin node keyed by suggestion, so
a card keeps its component whichever box it is in; the radio "Preview in document", Enter
and Space choosing), `DocumentPreview.tsx` (the items, built once per change: boxes and cards, notes, the
draft and the threads through the shared `useThreadsBesideView`; one expanded thread; the
margin when Comments is on or the file has suggestions, under the Comments panel's id
only while Comments is on), `FileView.tsx` (comment props to the rendered reading view; `SideThreads`
kept for Rich text editing, custom and deferred previews). Removed: the in-document grid
CSS, the `g5-note` tree nodes, and `FileView`'s reveal request to the rendered document,
which no longer had a sender. A Claude code review stood in for the Codex rounds (Codex
is not logged in on this VM); its findings were fixed in the same change.

Tests: `documentMarginLayout.test.ts`, `DocumentMargin.test.tsx` (order, push-down, the
expanded thread level with its block, Comments off/on, empty state, narrow row), the
updated `DocumentReviewPreview.test.tsx` (cards in the margin, one box per conflict set
naming the previewed reviewer, radios, decision identity and drafts across regrouping)
and `ReviewSuggestionOverlay.test.tsx` (notes in the margin). Browser flows updated to the
radio but not run in this VM (no stack): `document-independent-suggestions.spec.ts`,
`review-suggestions.spec.ts`; `comments.spec.ts` needs no change (the margin lists threads
in text order under the same ids).

## The Reviews panel: paragraphs with their changes (2026-10-31)

Source: the owner's [second request](../../chat_intent/kaihui-suggestion-mode-improvement/cards-out-of-document.md#second-request-the-reviews-panel-2026-10-31)
and choices (C, cards under the paragraph, the paragraphs around it), recorded on the
question set `reviews-panel-suggestion-context` in Project Docs.

Decision: in the Reviews panel (Approval page and the workspace's Reviews tab, one
component), the current commit's open text changes to a Markdown file are shown by file
and paragraph across every review: each edited paragraph once, with every reviewer's
changes marked inline as the document shows them, the paragraphs before and after it
greyed for context, and the decision cards under it; a connected set of conflicting
changes is one box with a radio on each alternative, as in the document's margin.
Each card names its reviewer and time. The review sections (reviewer, verdict, summary,
findings, decided items) stay below, and name how many of their changes are shown
above. Outdated commits' rounds, changes to non-Markdown files, comments, artifacts and
new files keep today's cards. While a file's text is loading, or when it cannot be
read, its changes are shown as today's cards under the file's heading, with a note.

Amended commitment: [review items](../../ontology/features/reviews.g5.md#g5.governance.review.items).
The display reuses [review placement](../../ontology/features/documents.g5.md#g5.documents.review-placement)
and [alternatives](../../ontology/features/documents.g5.md#g5.documents.review-alternatives)
unchanged: the panel reads the file as the document view does and runs the same
projection, so the marks, conflict sets and attention cases are the document's.

Owners and reuse: `ReviewFileSections.tsx` owns one file's section: it reads the file
(`useFileContent`), binds the digest (`useReviewSourceDigest`), projects and groups the
file's open changes (`projectSpecReviews`, `groupSpecReviews`, moved from
`DocumentPreview` to `specReviewProjection.ts`), renders each group's preview rows with
`SpecReviewSuggestions` between the neighbouring blocks, and adopts the cards and
conflict headings `SpecReviewSuggestions` renders into the panel's card nodes
(`reviewMarginItems`, `suggestionCardNode`, `conflictHeadNode`). `ReviewsPanel.tsx` decides
which changes a section owns (open text changes to Markdown files in the current commit
group), renders the sections above the review cards, leaves those changes out of the
review cards and the competing section, and keeps its card portals for the fallback.
Decisions, discussion, drafts and the deep-link reveal are unchanged: the cards are the
same `SuggestionCard` under the same ids.

Acceptance and tests: with two reviews editing one paragraph of a Markdown file, the
panel shows the paragraph once with both reviewers' marks, the neighbouring paragraphs
greyed, a box for the conflicting pair with a radio that changes the paragraph, separate
cards for the compatible changes, Accept from a card sending the same request as before,
and the review cards noting the changes shown above. An unreadable file shows today's
cards under the file heading with a note. `ReviewsPanel.test.tsx` covers these; the
existing panel tests keep passing (their changes are to non-Markdown files).

Implemented (2026-10-31): `ReviewFileSections.tsx` (the section, its fallback and the
paragraph groups), `ReviewsPanel.tsx` (which changes a section holds, the sections above
the review cards, the note on each review card, competing clusters left to the boxes),
`SpecReviewSuggestions.tsx` (`ReviewMarginItemSlot`, shared with the document margin),
`specReviewProjection.ts` (`groupSpecReviews`). Cards carry the reviewer's label, as
every card in the panel does; the section heading names reviewers by display name as
the review cards do. Tests: `ReviewsPanel.test.tsx` "the current commit's text changes by
paragraph" (marks, neighbours, conflict box and radio, review-card notes, no competing
section, Accept's request; the unreadable-file fallback). No browser fixture renders the
panel; the Approval page was not exercised in a browser in this VM.

Codex review rounds (2026-10-31, `gpt-6-astra`, high): twelve rounds; rounds one to
eight and ten found defects, fixed in the same PR, and rounds nine, eleven and twelve were
clean, the last two consecutively: a decision's notice kept with its suggestion; Accept
finding for sectioned changes; the margin re-measuring on content changes; the file read
through the workspace facade; paragraph headings opening at the paragraph's line and
links in paragraphs through workspace navigation; discussion open state handed between
a suggestion's card instances (and adopted by an already mounted one) while staying the
instance's own; a new draft set level with its line and taking focus without scrolling;
failed file refreshes said; the document column keeping its own height; changes between
paragraphs framed by their boundary; a deep link revealed once the sections have settled
and not replayed afterwards; a section that leaves releasing the reveal; a decision's
notice keeping no round in view; repository links in inline-rendered rows; an activated
card brought into view in the stacked margin.
CI's `frontend-test` step failed once on an unrelated onboarding-tour test under load
(`OnboardingTour.test.tsx`, "ends on the Guided step when the full sidebar's header is
activated by keyboard"); it passes locally and was not changed here.
