# Issue #925 — Core Ontology editor regressions after #943

Work for [G5Labs/G5-Lean#925](https://github.com/G5Labs/G5-Lean/issues/925) on
`issue-925-fix-core-ontology-editor-regressions-v2`. Scope and decisions:
[decisions](../../chat_intent/issue-925/decisions.md). The issue was written
before PR #943 (rich-text editing) merged; this plan reclassifies its items
against the code that includes #943. No backend, route, permission or store
changes.

## Governing commitments

Amended here (see the diff of `documents.g5.md`):

- [`g5.documents.preview.rich-editing`](../../ontology/features/documents.g5.md#g5.documents.preview.rich-editing):
  Rich text shows ontology prose only.
- [`g5.documents.comments.identity`](../../ontology/features/documents.g5.md#g5.documents.comments.identity):
  selection commenting in Preview as well as Source; no record Comment action.
- [`g5.documents.ontology.links`](../../ontology/features/documents.g5.md#g5.documents.ontology.links):
  targets open Preview from any view, once, with a brief highlight (R1, K1).
- [`g5.documents.editor.modes`](../../ontology/features/documents.g5.md#g5.documents.editor.modes):
  view switches keep Preview's reading state (K2).
- [`g5.documents.ontology.presentation`](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation):
  no permanent Human badge (O3); Rich text's prose-only display.
- [`g5.documents.ontology.creation`](../../ontology/features/documents.g5.md#g5.documents.ontology.creation):
  Rich text adds and deletes whole statements (2026-11-02; see
  [below](#rich-text-statement-add-and-delete--2026-11-02)), superseding the
  2026-10-29 decline recorded in its metadata.

Unchanged and relied on: [statement changes](../../ontology/features/documents.g5.md#g5.documents.ontology.statement-changes),
[All changes](../../ontology/features/documents.g5.md#g5.documents.ontology.all-changes),
[shared Markdown](../../ontology/features/documents.g5.md#g5.documents.markdown.shared),
[file links](../../ontology/features/documents.g5.md#g5.documents.file-links),
[comment authority](../../ontology/features/documents.g5.md#g5.documents.comments.authority)
and [comment drafts](../../ontology/features/documents.g5.md#g5.documents.comments.drafts).

## Item status after #943

Checked by reading HEAD `f226c1a08`; nothing was run.

- Still present, unchanged since the issue: O1–O4, R1–R8, K1, K2, G1a, G3, R11.
  #943 widened R1 (targets are also ignored in Rich text) and K2 (state also
  resets on Rich text ↔ Preview).
- Mostly done by #943: G2 (double-click opens Rich text at the word; the
  metadata footer still triggers it) and K3 (Mod+S in Rich text; nothing in
  Preview).
- Moot: R9 (pending text now joins the shared draft within 300 ms and on blur),
  R10 (no pencils; double-click on a Current row opens Rich text with Compare on,
  Before rows carry no block index) and K4 (no block editor).
- Dropped by the owner: O5, O6.

## Prose-only ontology Rich text

Owners stay as #943 set them: `markdownDocument.ts` decides what is opaque and
protected, `richExtensions.ts` owns the schema and the protected-line guard,
`RichMarkdownEditor` the node views and keys.

- **What hides.** In ontology files, the loader marks protected record lines
  hidden exactly when Preview hides them: `documentMarkup`'s hidden set (anchor,
  marker, identity heading) for statements without a diagnostic, and
  `disclosedCurrentMetadataIndices` for paired metadata and the Metadata heading.
  One rule therefore decides both views; a diagnostic or unpaired entry stays a
  visible opaque source block, as now.
- **Hidden, not removed.** Hidden lines remain protected opaque nodes rendered
  with no box; serialization and the guard are unchanged, so a no-edit save stays
  byte-identical and no edit can delete or duplicate them.
- **Statement grouping.** A decoration plugin derives each statement's range from
  its hidden marker node to the next statement's first hidden node or section
  heading — the same boundary `currentRecordSpans` uses — and decorates its body
  nodes so CSS draws Preview's bullet and hover/focus outline. Derived per
  transaction; no stored state.
- **Keys.** Backspace at the start of a statement's first body block and Delete
  at the end of its last are consumed with no change. A selection landing on a
  hidden node moves on in the direction of travel. Enter at a body's end adds a
  paragraph to that statement, matching Source's boundary rule.
- **Entry.** A double-click or target offset at a statement's marker opens Rich
  text at its first body block.
- **Assumption to verify in the browser:** ProseMirror's arrow, selection and
  IME behaviour around zero-size atom nodes. If skipping proves unreliable, the
  fallback is a one-pixel non-interactive node, not visible source.
  Verified in Chromium: hidden nodes render with `display: none` and arrow
  skipping, boundary keys and IME composition work, so no fallback is used.
  Arrows that cross a boundary land at the neighbouring block's start or end
  rather than keeping the caret's column.

## Commenting

Items G1–G3 and R11.

- **Source (G1a).** `CodeEditor` reads Monaco's selection and model text for the
  Comment button position, quote and line (on Monaco's mouse-up). Right-click
  uses Monaco's own context menu, which stops the event before React sees it,
  so Add Comment is a Monaco action there, enabled by a context key while the
  person may comment. The HTML loading viewer keeps its DOM selection path.
- **Preview (G1b).** `DocumentPreview` offers Comment for a non-empty selection
  inside current content: not in Before or deleted rows, review or question
  cards, the metadata footer, or All Changes. The line is the first source line
  of the selected text, found within the block's source range, else the block's
  first line. The draft opens in `SideThreads`, which gains a new-thread composer
  reusing `DraftCommentInput` and the existing `onAddComment` handler; no new
  API. The open/send logic moved from `CodeEditor` into `useNewThreadDraft`,
  keyed by the file's existing composer subject, so Source and Preview show
  one draft per file that survives view changes.
- **Gate (R11).** Preview offers Comment exactly when Source would. A reader
  composing a suggestion is reading unpushed text, which only the owner may
  anchor, so neither view offers it then.
- **G3.** The record `navigationControl` Comment button and FileView's
  `commentAtLine` are removed.
- **Rich text** offers no commenting (agent choice; Preview and Source own it).

## Navigation (R1–R4, K1, K2)

- **R1.** `FileView` treats a new statement/heading target (and one present at
  mount) like a line citation: it flushes Rich text and shows Preview. Line
  targets keep opening Source.
- **K1.** `DocumentPreview` resolves each `(target, seq)` once: it retries while
  content or the comparison is loading so a deleted statement's Before row can
  be found, then scrolls, highlights for 2 s and marks the request handled, or
  shows Missing target.
- **R2.** `CurrentOntologyReference` mounts only for ontology files.
- **R3, R4.** Preview resolves fragments through `ontologyFragmentIndex` (the
  rule the popover and the server's `current_ontology_anchor_counts` share) to a
  block index, rather than through `rehype-slug` DOM ids; a target naming an
  identity heading resolves to its statement, and a zero-size marker carries the
  heading's block index for the outline. The outline lists identity headings by
  kind, as the pre-unified editor did, since outlines never show statement
  titles. `rehype-slug` and DOM ids remain the fallback and serve ordinary
  documents.
- **K2.** Compare, filters, open details and scroll position are held by
  `FileView` for its mount and handed to `DocumentPreview` (`readingState`),
  which `SemanticRecordFrame` follows for open details. `FileView` stays mounted
  across Source, Rich text and Preview, so no session store is needed, and a
  newly opened document still starts with Compare on. The same object records
  the last navigation request Preview handled, so a remount neither resets the
  state nor jumps again, and `FileView` switches views only for an unhandled
  request.

## Presentation (O1–O4, R5–R8)

- **O1.** The toolbar gets button styling, Compare's selected state and a divider
  before the filters, after the pre-unified `SpecEditor` toolbar.
- **O2, O4.** One comparison-pair component serves per-file Preview and All
  Changes: the bullet sits on the pair; labels, backgrounds, outline and metadata
  start after it; Before uses the same bullet; the footer meets the body. All
  Changes drops its extra row padding and spacing.
  Corrected 2026-10-30 by the owner: the pair has exactly one bullet and its
  Current and Before rows meet with no gap, as All changes showed before #925;
  the first implementation repeated the bullet on each row.
- **O3.** `SemanticRecordFrame` no longer renders the Human badge; the filter and
  details are unchanged.
- **R5.** The popover body and details take the document link styling.
- **R6.** The last-child margin reset targets the statement block grid; stale
  block-edit selectors go.
- **R7.** All Changes passes each file's workspace, repository and path to
  statement Markdown and metadata.
- **R8.** Compare's title is neutral while off and says unavailable when the
  comparison cannot run.

## Remaining G2 and K3

- The metadata footer joins the double-click exclusions.
- Mod+S in Preview while editing requests the ordinary Save for an owner (writing
  pending Rich text first, as Save does) and only prevents the browser dialog for
  a reader or in comment and question inputs.

## Verification

Each item gets a regression test. Component tests (Vitest) cover O1, O3, R2, R5,
R7, R8, R11, the comment gate and exclusions, the hidden-line rule and K2's
store. Browser tests use the mock-preview fixture with real Monaco and Tiptap:
G1a selection and right-click comment in Monaco; G1b selection comment with its
anchor line; prose-only Rich text (hidden lines, bullets, Backspace/Delete at
boundaries, arrows, a no-edit save byte-identical, an edit saving only its
block); R1 from Source and Rich text; K1 single jump with highlight across a
later edit; K2 state across Source and Rich text switches; O2/O4/R6 layout in
light and dark themes; G2's footer exclusion; K3's Mod+S. Existing
`file-editor`, `markdown-layout`, `OntologyComparison`, `CurrentOntologyPreview`
and `EditorComments` suites must stay green.

## Implementation and verification — 2026-10-29

Implemented on this branch: commenting, navigation, reading state, G2 and K3
in the main checkout; prose-only Rich text and the presentation items in two
worktrees, cherry-picked here. Results at the integrated head:

- Vitest over `components/files`, `components/editor`, `components/comments`,
  `lib` and `pages`: 155 files, 2,279 tests pass. New cases include the
  selection's source line (`previewSelection.test.ts`); Preview selection
  Comment, its exclusions and gate, targets resolved once with a highlight,
  punctuation and identity-heading targets, the kind-labelled outline, reading
  state across a remount, the details-footer double-click exclusion and the
  Project Doc line link (`DocumentPreview.test.tsx`; the R2 case fails without
  its fix); FileView targets from Source and Rich text, the shared draft sent
  from Preview, no Comment for a suggesting reader, and Mod+S
  (`FileView.test.tsx`); and the agents' Rich text and comparison-pair suites.
- Browser, real Monaco and Tiptap (mock-preview `file-editor` and
  `file-editor-925`): 31 tests pass, including Monaco selection and context-menu
  Add Comment, Preview selection Comment at its line, targets from Source and
  Rich text, Compare/filters/scroll across switches, prose-only Rich text
  (hidden lines, bullets, exact Save bytes, boundary keys, arrows, synthetic IME)
  and comparison layout measured in both views. The frontend-only
  `e2e/browser` suite (74 tests, including `markdown-layout`) passes.
- `tsc`, ESLint (no errors) and Prettier on changed files pass.

Not run here: the full-stack flows `current-ontology` and
`ontology-comparison`, which now comment through a selection instead of the
retired record button; CI runs them. Known limits: arrow keys crossing a
statement boundary in Rich text land at the next block's edge rather than the
caret's column; IME was checked only by synthetic composition in Chromium;
Backspace at the start of a list that opens a statement lifts it into a
paragraph inside the statement before stopping; the popover's statement
Markdown still has no file context for relative images (outside R5).

## Rebase onto one-surface editing — 2026-10-30

Base's [one-surface decision](../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30)
(PR #967) removed the Preview, Source and Rich text tabs: a Markdown file is
one rendered document, read in place and edited as Rich text, with a switch to
plain text, and Done returns to the reading view with the unsaved draft and
Save kept. Its [comment list](../../chat_intent/workspace-colab-harden/comments-side-list-in-source.md)
changes ordered threads by line and let a card's line label open plain text.
No human decision on either side contradicts the #925 decisions, so each #925
commitment keeps its intent in the new vocabulary:

- **R1, K1.** A statement or heading target opens the rendered reading view
  from either mode and leaves editing as Done does (Rich text flushed, draft
  kept).
- **K2.** Switching modes or entering and leaving editing keeps the reading
  view's Compare, filters, open details and position.
- **K3.** Mod+S outside the editors saves whenever Save is offered, so also in
  the reading view after Done, and otherwise only keeps the browser's dialog
  away, as it also does in comment, question and other text inputs.
- **G1, G3.** Selection commenting stays in the reading view and plain text;
  base's per-record Comment jump (`commentAtLine`) goes with the record action.
  Base's line label, card ordering and reveal-to-top are kept. Base's sentence
  that the rendered document has no selection toolbar was text carried from
  before #925, not a new decision, and is replaced.
- **R11.** Done can now show a reader their unpushed suggestion while reading,
  so a reader gets no Comment while editing or while their draft is shown, in
  either mode ([comment authority](../../ontology/features/documents.g5.md#g5.documents.comments.authority)).
- Prose-only Rich text, the comparison pair, the toolbar and the Human badge
  are unchanged by the rebase.

Base's 8-inch [column](../../ontology/features/documents.g5.md#g5.documents.text-measure)
centres each top-level block, which overrode a changed pair's indent; a changed
or deleted pair now sits in a plain block like an unchanged statement, so its
bullet lines up with theirs.

Verification after the rebase onto `dd848a279`: `tsc` and Prettier pass; Vitest
5,567 of 5,567 after one stale "Source" notice assertion was corrected;
mock-preview `file-editor` and `file-editor-925` 34 of 34 in Chromium; the
`e2e/browser` suite 68 of 74, where the six failing `markdown-layout`
full-width cases fail identically on base alone (they predate the 8-inch
measure). ESLint's two errors in `e2e/fixtures/file-editor.tsx` also exist on
base. Not run: the full-stack flows, as before.

Full-stack check, 2026-11-02 (preview seeded with G5-Lean at the branch's base
plus this branch's `documents.g5.md`): every #925 item behaved as designed
except K1 from Rich text, whose 2 s highlight vanished after about 0.5 s
because the reading view re-rendered after the draft was written. The reading
state now holds the highlight's target and end, and every render reapplies it
to the element that holds the target (`DocumentPreview.test.tsx`).

## Reading tools in the file row — 2026-11-02

At the owner's [instruction](../../chat_intent/issue-925/decisions.md#human-decision--reading-tools-in-the-file-row--2026-11-02),
Compare and the filters leave the top of the document for the file row, which
[editor.modes](../../ontology/features/documents.g5.md#g5.documents.editor.modes)
already makes the only toolbar. `DocumentPreview` keeps owning their state and
the comparison query and renders them through a portal into a place FileView
reserves in the row while the reading view shows; without one (its own tests)
they stay above the document. They come first among the row's controls, with a
divider before the mode switch, and their bordered buttons take the row's 24 px
height. Out of the scrolling document, they no longer scroll away (one of the
items deferred to #927). Tests: `FileView.test.tsx` (in the row, before the mode
switch, gone in plain text and while editing) and the browser specs.


## Rich-text statement add and delete — 2026-11-02

At the owner's [decisions](../../chat_intent/issue-925/decisions.md#human-decisions--2026-11-02),
[statement creation](../../ontology/features/documents.g5.md#g5.documents.ontology.creation)
lets Rich text add and delete whole statements. Owners stay as before:
`richExtensions.ts` the schema, the structure guard and the keys;
`markdownDocument.ts` the loader; `RichMarkdownEditor` the line ending and
notices. No backend, route or store change. Deleting checks nothing beyond the
open document ([no link check](../../chat_intent/issue-925/decisions.md#human-decision--no-link-check-on-delete--2026-11-02)).

- **Statement unit.** A statement's protected lines, derived from the document
  on demand: its identity run (the hidden anchor, identity-heading and marker
  nodes ending at the marker) and its metadata entry (the hidden `### ID` node
  and the hidden nodes after it, up to the next heading of level 3 or above).
- **Guard.** `ProtectStructure` admits a change when protected lines outside
  units are unchanged and each unit is unchanged, added complete or removed
  complete. This contains the old multiset rule when no unit changes, lets undo
  and redo of an add or delete through, and still refuses a selection that
  would take a marker without its metadata or the Metadata heading.
- **Add.** Enter with the caret at the end of a nonempty top-level paragraph
  that is a statement's last body block runs one transaction: after that block a
  hidden marker node (`<a id="user.U"></a>` and `[I · user.U]` on two lines, in
  the file's line ending) and an empty paragraph; after the statement's metadata
  entry, hidden `### user.U` and `- human-provided` nodes; the caret in the new
  paragraph. New nodes have no baseline entry, so they write their source with
  blank-line gaps while untouched blocks keep their bytes. Elsewhere Enter is
  unchanged; on an empty statement it is consumed.
- **Shift+Enter** in a statement's top-level paragraph splits it into a new
  paragraph of the same statement (Enter's former meaning there); elsewhere it
  keeps its line break.
- **Empty statements.** A statement is empty when its body is one empty
  paragraph. The loader gives a bodiless statement (its marker directly followed
  by a record line, a heading or the end) an empty paragraph, which writes
  nothing, so a statement left empty, or authored without prose, can still be
  typed into or removed after a reload. A decoration marks it, and CSS shows the
  placeholder.
- **Delete.** Backspace in an empty statement removes its identity run, body
  and metadata entry in one transaction, at once; links to it elsewhere are
  left as written. A first version read every ontology file for links and
  asked in a dialog; in the preview that took about 3 seconds per press and
  was dropped at the owner's decision.
- **Refusal.** Add and delete show a notice pointing to plain text when a
  statement's record lines show for repair (a diagnostic), an unpaired `### ID`
  entry shows, the Metadata heading is missing, or the statement has no
  metadata entry. The Metadata heading also shows when the section holds
  ordinary prose (`root.g5.md`, `agent_approvals.g5.md`); that is not a
  repair. Treating it as one refused every add and delete in those two files,
  which the owner found in the preview on 2026-11-02.
- **Limits and assumptions.** A reader's add or delete becomes separate
  suggestions, down to separate edits within one (deferred grouping): accepting
  only part leaves lines for repair in plain text.
- **Tests.** Component: add (written text, metadata placement, CRLF, caret,
  refusal), Enter in lists and on an empty statement, Shift+Enter, the guard
  (undo and redo, partial removal refused), placeholder, delete at once and
  undo, delete of a linked statement, refusal while lines need repair, the
  loader's empty body staying byte-identical. Browser (mock-preview, real
  Tiptap): Enter, type, Save writes the expected file; typing then erasing and
  one more Backspace deletes; Compare shows Added.
- **Verified (2026-11-02).** First version, with the link check: component
  and browser tests passed, and in the full-stack preview (owner and reader)
  every step behaved as designed, but the check made Backspace take about 3
  seconds there. After dropping it, see the results below.
- **Verified without the check (2026-11-02).** Component tests pass (files,
  editor, lib and hooks suites, 2051 tests) and all 37 `file-editor` browser
  tests pass. In the rebuilt full-stack preview, Backspace deleted a new empty
  statement in 28 ms and an emptied existing statement that two other files
  link to in 23 ms, with no file reads; Compare showed it as Deleted, and
  Discard left the file unchanged. The boundary-keys browser journey failed
  about one run in ten after the add/delete journey: a key pressed right after
  a click or a caret key acted before ProseMirror read the browser's
  selection, the browser edited across a hidden line and the guard refused it
  (nothing lost). It now places the caret through the editor before each
  Backspace and Delete: 20 of 20 paired runs and two full `file-editor` runs
  passed. The mock-preview suite is not in CI.

## Rebase onto main — 2026-11-03

Main had moved 244 commits, and while #925 was in progress it built its own
rendered-document commenting from the owner's decisions of 2026-10-30 (a
Comment bubble on selected text, one new-thread draft per file) and 2026-10-31
(suggestion cards and comments in a margin level with the text), plus image and
PDF viewers. Reconciled under
[comments.identity](../../ontology/features/documents.g5.md#g5.documents.comments.identity):

- **Main's implementation is the one kept** for the bubble, the draft
  (`commentDraft.ts`, `SelectionCommentBubble`) and the margin; #925's
  `useNewThreadDraft`, `previewSelection` and side-list draft go. The margin is
  the later layout decision; #925 only described the side list of its time.
- **#925 keeps on top of it**: Monaco's own selection and Add Comment action in
  plain text (G1a; main still read the browser's selection there), no
  commenting in Before and deleted rows or on All Changes, the reader gate
  after Done (R11), and no record Comment action (G3, the owner's 2026-10-28
  decision; main's text only mentioned the action in passing).
- **Main's ontology changes card** keeps its Edit control: the record frame's
  action slot, which #925 had dropped as unused, is back, and the card's
  changed statements render through the shared comparison pair, with main's
  unchanged-row styles kept. All Changes passes each statement's file, as R7
  needs, beside the card's own link context.
- Everything else of #925 applied as written: the file row toolbar, targets,
  reading state, Mod+S, prose-only Rich text and statement add and delete.
- **Verified after the rebase.** Typecheck and lint pass; all 5810 Vitest
  tests pass. In Chromium, all 37 mock-preview `file-editor` tests pass (two
  #925 journeys now follow main's margin draft and its inner scroll container),
  and the `e2e/browser` review-suggestions, project-doc and file-viewers specs
  pass. Six `markdown-layout` chat-width cases fail the same way on main.

## Review fixes — 2026-11-03

- A file opened at a statement or heading target starts reading, with its
  draft kept, even if it was left editing: before, Rich text opened once it
  became available after the target was shown (for example when the
  workspace started). `FileView.test.tsx` covers it.
- The loader gives a statement an empty body only when a heading that ends
  the statement follows its marker, so a body that opens with a lower
  heading has no stray empty paragraph (`markdownDocument.test.ts`).
- A new statement's ID uses `randomUuid`, which also works outside secure
  contexts; a spent target highlight is cleared so later renders stop looking
  up its target; the outline finds identity headings by index.
