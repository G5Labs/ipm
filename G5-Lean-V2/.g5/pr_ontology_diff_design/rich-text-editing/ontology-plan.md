# Edit Markdown blocks in Preview

> **Superseded on 2026-10-29.** The per-block source editor this section
> designed (PR #871) was retired by the owner's
> [rich-editing decision](../../chat_intent/rich-text-editing/request.md#human-decision--rich-editing-in-preview--2026-10-29)
> and replaced by the Rich text view in
> [Rich editing in Preview — 2026-10-29](#rich-editing-in-preview--2026-10-29)
> (PR #943). The statements `g5.documents.preview.block-editing` and
> `.commit` no longer exist; the text below is kept as the record of that
> work and its review.

Work for the owner's [design](../../../docs/preview-block-editing-design.md)
on the assigned `rich-text-editing` branch, authorized by the
[request](../../chat_intent/rich-text-editing/request.md). The design is the
product decision; this plan records ownership, the guarantees relied on and the
choices the design left open. No backend route, permission or store changes.

## Governing decisions

Block editing (`g5.documents.preview.block-editing`, retired) and its commit
rule (`g5.documents.preview.block-editing.commit`, retired) were new; they narrow [editor modes](../../ontology/features/documents.g5.md#g5.documents.editor.modes),
[shared Markdown](../../ontology/features/documents.g5.md#g5.documents.markdown.shared)
and [ontology presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation),
and leave [editor ownership](../../ontology/features/documents.g5.md#g5.documents.editor.ownership),
[drafts](../../ontology/features/documents.g5.md#g5.documents.files-view.drafts),
[the source engine](../../ontology/features/documents.g5.md#g5.documents.editor.engine),
[opening defaults](../../ontology/features/documents.g5.md#g5.documents.source-editing),
[reader suggestions](../../ontology/features/documents.g5.md#g5.documents.reader-suggestions),
[review placement](../../ontology/features/documents.g5.md#g5.documents.review-placement)
and [selected Save](../../ontology/features/documents.g5.md#g5.documents.selected-save)
unchanged. PR #849's fit-width surface is a precondition; this branch is stacked
on its last substantive commit and is rebased onto `main` after #849 merges.

## Ownership

- `blockEdit.ts` (new, pure): the block's source text and line-ending shape,
  the splice, the block-parser and renderer-parser guards and the
  editable-index rule. It never touches state; it is the one place that knows
  which bytes a block owns.
- `DocumentPreview` owns the one open block editor per file: which block, the
  text typed so far, the original it started from and the last refusal. It
  wraps each editable rendered element in a `g5-block` host after the review
  plugin, keeps `data-block-index` on the rendered element so the outline,
  review placement and comment anchors are unchanged, and asks FileView to
  write through `onEditBlock(expected, next)` gated by `canEditBlocks`.
- `fileDrafts` retains the pending block text with the browser-tab session
  beside the selected view, so navigation keeps it; a person change disposes
  it with the session and Discard clears it explicitly.
- `FileView` decides authority: `rewriteDraft` generalizes the owner-only
  preview rewrite to the viewer's role under the same `mayEdit` gate the Edit
  and Suggest buttons use, plus not pushing and no Save in flight. It also
  commits the open editor before Save, Push review, Edit and a switch to
  Source, and cancels it on Discard.
- `CodeEditor` gains `onBeforeSave`: Save's buffer and session are its own, so
  only it can run the commit first and then save the adopted draft. Its model
  adoption of a draft changed elsewhere is unchanged and supplies Undo.

## Operation

Open: the pencil of block `i` (a top-level `parseMarkdownToBlocks` block that
the preview rendered with `data-block-index`) shows a textarea with
`blockSourceText(content, block)`: the exact source range with `\r\n` shown as
`\n`. Opening and Cancel change nothing. Done with unchanged text closes.

Commit: `spliceBlock(content, block, edited)` restores the block's `\r\n` and
trailing `\r` when it had them, then `untouchedBlocksPreserved` re-parses the
result with the same ontology flag and requires every block before `i` and every
block after the edited region to match the old blocks one-to-one by type and
source text. A failure (typically an unclosed fence swallowing what follows, or
a list that merged with its neighbor) keeps the editor open with the reason.
Success calls `onEditBlock(content, next)`, which is `writeFileDraft` with the
expected content: it refuses a changed draft or in-flight Save, starts the
owner's or reader's draft with the file's baseline, sets `editing` and keeps
Preview. The retained Monaco model adopts the splice as one native edit, so Undo
in Source restores the block. Save, Push review, conflicts and Compare follow.

Save-first: CodeEditor's `handleSave` runs `onBeforeSave`; FileView's hook
commits the open editor and returns false on refusal, which cancels the Save.
The buffer adopts a draft written by the hook only on a later render, so when
the hook wrote this session's draft the handler submits that text directly;
when it started a draft on a clean buffer it adopts the session and baseline
first, as the mirror would. Save stays available while an editor is open
(`pendingEdit`), so pending text alone is savable. Without a hook write the
Save path is unchanged. The same flush precedes the push panel's submit (which
rereads the resulting draft), Edit, the Source tab, a citation jump, a record
Comment and opening another block; a refused commit cancels the action and the
pending text stays. The open editor's placement follows the rendered tree: the
preview plugin wraps the pending block's element where it renders, and when no
element renders for it (removed, changed, filtered, compared or dropped by
sanitization) it appends a recovery host at the end of the document, so pending
text always has a visible editor and Cancel. Each opening is a distinct editing
session, so a successor shifted onto a deleted block's index gets a fresh editor.

The block parser and the preview's remark pipeline group lines differently for
HTML blocks, fences with unusual info strings, nested list content and lazy
continuation, so an edit the block guard accepts could still make Preview
swallow what follows (an open comment, `<script>`, `<div>` or fence). The
commit therefore also parses the old and the spliced document with the
renderer's own parser (`remark-parse` with GFM and math, now declared
dependencies) and requires every top-level node outside the region to read the
same: nodes before it unchanged, nodes after it identical and shifted by the
edit's length, and a node containing the region (a list item holding the
edited paragraph) keeping its type and start, ending exactly that much later,
with its children held to the same rule; a node that begins inside the region
and reaches past it (a list whose first item is the edited block) must do the
same afterwards. After remark, the preview parses raw HTML with a browser
parser whose rules for misnested tags, comment terminators, raw-text states
and attributes no re-implementation tracks reliably (review passes five to
fifteen each found a new divergence), and whose tree keeps no usable
provenance for the text it re-creates. Block editing therefore admits only raw
HTML that cannot change that parser's state for other blocks: inline elements
(`a`, `b`, `code`, `em`, `kbd`, `span`, `sub`, `sup` and the like) and simple
block elements (`p`, `div`, `details`, `summary`, `section`, `figure`,
headings and the like, which the parser inserts and pops as plainly as inline
ones when nested strictly, closing only an open paragraph on the way; not list
items, whose opener closes an open item through any inline element) opened
and closed within the same Markdown element, the inert void elements `br`,
`hr`, `img` and `wbr`, and simple comments, recognized by a grammar that uses
only HTML's ASCII whitespace and must consume everything remark reads as HTML.
Table, select, raw-text and foreign elements are never read: they change the
parser's mode.
Balance is checked per Markdown parent, not across the block: the elements
Markdown generates between two raw tags scope them (a table cell ignores a
closer for an element opened outside it and the opener stays in force after
the table; emphasis is reconstructed around a closer outside it), so `<b>`
before a table and `</b>` in one of its cells is not balanced where it is read.
Each parent's html nodes are read as one sequence over the whole document, not
the block alone: an unclosed `<pre>`, `<textarea>`, `<script>` or `<style>`
makes remark read everything after it as that HTML block, so a block inside it
is HTML however its own text looks. A block is safe when the elements the
grammar sees open before its HTML are the same ones open after it (so `<b>`
across several blocks may stay open, but no block may close it). A block with
anything else (a `<div>`, `<details>`, `<table>`, `<script>`, an unusual
comment, a pair crossing elements) gets no pencil while rendered content
follows it, and an edit that introduces such HTML is refused. Where an earlier
block holds HTML beyond that set the parser's state at the block is unknown;
the Markdown constructs that would change such a state are few and known (a
generated table closes a raw table or a raw select inside one; a link,
autolink, footnote reference or raw `<a>` closes a raw link, except inside a
table cell, whose marker shields it; a top-level heading pops a raw heading; a
task checkbox closes a raw select), so for the tag names that earlier HTML
mentions from the point the grammar lost track, the document may not gain or
lose those constructs (raw openers of links and paragraph-closing blocks in
the edited HTML count with the generated ones; whether a heading pops an open
raw heading depends on what the parser has open at that moment, so under that
sensitivity the edited region's top-level structure, node types and raw tags
as tokenized, must hold instead): in place, as whether the edited region
places each construct at all, under the state at the block, and in
the footnotes section, counted per referenced footnote in the order the
section lists them under the state at the end of the document, where the
section renders. An open raw paragraph (or foreign content) is closed by the
next top-level block, so there the number of top-level blocks must hold as
well: deleting a block, or replacing it with a definition that renders
nowhere, would leave an inline HTML block after it inside that paragraph. A
link reaches backwards as well: closing a raw link re-parents the element the
link is in, which may be a raw division holding untouched earlier blocks, so
the link count holds even for a last block; and a raw table holds its text
until a token follows it, dropping it at the end of input, so under the table
sensitivity something must still render from the block on, for a last block
too. Raw tags are counted as the grammar tokenizes them (a comment or an
attribute value is not a tag), and the raw sets follow the HTML parser's own
lists. A last block otherwise hides nothing and is
accepted, unless the document has footnote definitions: the renderer places
the footnotes section after the last block, where an unclosed element would
swallow it, so footnotes count as content following every block. This narrows
the design's "HTML blocks" to the self-contained subset; the owner can relax
it, with Source as the way for the rest.

## Eligibility

Editable: blocks with a `sourceRange` whose rendered element carries that
block's `data-block-index`, including prose inside an ontology statement.
Not editable: front matter; indices in `markup.hidden` (current anchor, marker,
heading and metadata lines); a legacy record's label paragraph at
`spanStart`; `g5-question` and `g5-question-ref` fences; every block while
Compare is on or hidden by the Human or Risk filters (main retired the
Unimplemented filter with the legacy review metadata); blocks
covered or anchored by an open hunk projection or an anchored note (they keep
Accept/Reject and placement); and any rendered element the two parsers did not
agree on. The pencil is also absent while the person may not edit or suggest,
while a push or Save is in flight, and while the preview is deferred.

## Layout

The pencil is an absolutely positioned overlay in the `g5-block` host, placed in
the 24px right gutter of the Markdown surface; it reserves no width and must not
overflow. Inside a statement frame the action column (Comment, info) ends at the
body's right edge, so the frame measures that column into a CSS variable and the
pencil is offset past it, into the same gutter, whatever controls the column
holds; ontology list mode adds the frame's inline padding. The editor
replaces the block's element in the same surface with `width: 100%; max-width:
none; min-width: 0; box-sizing: border-box; white-space: pre-wrap`. The host is
an ordinary block so margins collapse as before; first/last-child margin rules
are forwarded to the wrapped element. No width cap is added and the
`document-preview`, `document-preview-content` and `.g5-markdown-preview`
structure measured by `markdown-layout.spec.ts` is unchanged; that spec gains
the open-editor width case the design asks for.

## Capacity and assumptions

Block editing needs the rendered preview, so the 128 Ki code-unit deferral and
all read/suggestion limits are unchanged. Each Done re-parses the file once,
which is the same cost as the preview's own parse. The guard is strict by
design: an edit that changes the neighbors is refused rather than reinterpreted.
Pending block text is in-memory session state, like Monaco models; closing the
browser tab loses it as it loses Source history. The 2026-10-03 experiment files
are not in the repository; the unit acceptance substitutes six repository
Markdown files and three ontology files, which exercise front matter, tables,
fences, HTML, records and CRLF through the same properties.

## Verification

Unit: `blockEdit.test.ts` on the nine files (no-op identity, one-word edit
locality on every editable block, unclosed fence refused, CRLF retained, a block
emptied leaves surrounding bytes). Component: `DocumentPreview.test.tsx`
(buttons only on editable blocks for an eligible person; none on front matter,
record identity lines, question fences, filtered blocks or blocks under an open
suggestion; Done with no change makes no call; a refused commit keeps the editor
open) and `FileView.test.tsx` (a reader's Done starts a `suggest` draft, an
owner's Done starts an owner draft in Preview, Save commits an open editor
first). Browser with real Monaco (`e2e/mock-preview/file-editor.spec.ts`): edit a
paragraph in Preview, Save, PUT body differs only in that block; Source Undo
restores it; Push review carries a block edit; a person change drops the draft
and pending text; the Save-conflict path is unchanged; the layout spec case. The
existing FileView, DocumentPreview, OntologyFileDrafts and workspace page suites
pass unchanged.

## Self-review

The design's commit path is the existing guarded draft write, so there is no
second session, Save API or authority. The one added mechanism beyond the design
is the pre-save hook, required because Save and its submitted buffer live in
CodeEditor; wrapping FileView's `onSave` instead would save bytes CodeEditor did
not submit and desynchronize baselines, and splicing on every keystroke would
re-render the document per key and fragment Undo. Retaining pending text in the
session is the minimal way to keep the design's "Save commits it first" true
across the views that unmount the preview. The strict neighbor guard and the
exclusion of review-anchored blocks keep review placement and question
authority intact without new rules. Opening or cancelling an editor is pure,
so initialization never dirties a draft.

## Implementation notes — 2026-10-27

Implemented as designed, with these details decided during implementation:

- `blockEdit.ts` restores a CRLF block's final `\r` even for empty text, so
  deleting a block leaves `\r\n\r\n` rather than a bare `\n`. A record's
  first block is excluded for current records too: the markup plugin drops the
  marker line without listing it in `hidden`.
- The open editor's text lives in a ref and the file session, not React state,
  so typing never re-parses the document; open, close and refusals re-render.
- CodeEditor's `handleSave` runs `onBeforeSave` and, when the hook wrote this
  session's draft, submits that text directly: the buffer adopts the store only
  on a later render, and a real-browser run showed a one-tick wait saving the
  pre-commit buffer. Without a hook write the Save path is unchanged and
  synchronous, which four existing in-flight Save tests require.
- An unclosed fence in the file's last block swallows nothing and is accepted;
  the guard refuses only when neighbors change.
- The browser fixture names the signed-in person in its API token: mock auth
  installs an opaque token, and a reader's Push review for a principal is
  refused under a token that does not name it.
- Opening another block commits the open editor first and then addresses the
  clicked block in the document that commit produced: a commit that splits or
  merges blocks shifts later indices, so the commit returns the re-parsed
  blocks and the next editor opens on the intended block with its real
  original. Done and Cancel return keyboard focus to the block's pencil.

Verification on the final head (rebased onto `main` after #849 merged):
`blockEdit.test.ts` (65 cases: the nine-document corpus, every raw-HTML rule,
and the renderer differential test), `DocumentPreview.test.tsx` and
`FileView.test.tsx` pass; TypeScript over sources and tests, Prettier and
ESLint (pre-existing warnings only) pass. Real Chromium: all 21 file-editor
journeys including the seven added (exact CRLF splice and Save body, Source
Undo, Save-first and a refused commit, clean-buffer Save of pending text,
reader Push body, person change, conflict path, ontology statement prose), and
the layout spec's new open-editor width case with #849's 25 cases, 40
layout/review/copy cases in all. Chromium measurements: the pencil occupies
the 24px gutter beside a full-width block in Docs and ontology list mode, on a
different row from a legacy record's info button and beside it for a current
statement. The full unit run passes all 5,281 cases across 343 files, and the
production build passes. An early full run had failed the `--g5-*`
design-token contract on the measured action-column variable, fixed by naming
it `--record-actions-width`. Published-head CI results are recorded on the PR.

## Codex review disposition — 2026-10-27

In short: twenty-one passes of the pre-PR Codex review (gpt-6-astra,
extra-high effort), each probing the guards through `applyBlockEdit`, the
renderer and the Preview Done path, until the twenty-first approved. The first
pass fixed editor-state defects; the rest concerned raw HTML, where the policy
moved from proving the renderer's HTML tree unchanged (canaries, provenance)
to the whitelist the Operation section describes, with context sensitivities
added as each pass found a construct the HTML parser treats specially. The
corpus limitation (the 2026-10-03 experiment files are not in the repository)
remains the one open P2. The per-pass record follows as lineage for the rule's
shape.

The first pass requested changes with seven P1 and six P2 findings; each was
reproduced against the code before fixing.

1. P1 pending text dropped when its block vanished: the commit now refuses and a
   recovery editor renders below the document; Cancel is the person's decision.
2. P1 a remounted editor showed stale text while Done submitted the typed text:
   typed text now lives in the file session and every mount reads it.
3. P1 the push panel's submit did not flush: `submitPush` commits first and
   rereads the resulting draft, including Push anyway.
4. P1 an unclosed HTML block opener hid later blocks: the guard refuses it when
   blocks follow (openers inside fences and closed openers pass).
5. P1 Save unavailable or not adopting a hook-created draft when pending text
   was the only change: `pendingEdit` keeps Save available and `handleSave`
   adopts the started session before persisting.
6. P1 citations and record Comments left Preview without a flush: both commit
   first; a refusal keeps Preview.
7. P1 a CRLF file's unterminated last block gained LF: a block without line
   endings of its own follows the file's style.
8. P2 repeated refusals lost their message: refusals are counted so the same
   reason shows again. 9. P2 opening another block stole focus: focus returns
   to a pencil only after a terminal Done or Cancel. 10. P2 the Outline lost an
   edited heading: the in-place editor carries the block's `data-block-index`.
   11. P2 the last-child margin rule targeted the pencil: it targets the
   rendered element. 12. P2 test strength: the corpus edit is a known-valid
   edit per block type that must succeed, and regressions cover items 1–7.
   13. P2 the Operation section described a superseded Save algorithm: updated.

Not changed: the 2026-10-03 experiment files remain unavailable; the substitute
corpus stands as documented.

The second pass found eight items resolved and four P1 remainders, each
reproduced and fixed: a block the renderer drops (an HTML comment) stranded the
editor invisibly, so placement now follows the rendered tree with an appended
recovery host; a successor shifted onto a deleted block's index reused the
emptied textarea, so editors are keyed by editing session; the HTML scan took a
fence inside a comment as Markdown, accepted `</pre >` and refused a `<script>`
line inside a closed comment, so it now walks lines with exact closers and skips
closed blocks; and a citation into a file whose session held a pending edit
opened straight into Source, so such a file opens in Preview, commits, then
follows the citation. The corpus limitation (P2) stands.

The third pass found three P1 remainders, each reproduced and fixed: a note
wrapper inside a record the filters hide counted as a place for the editor, so
the plugin no longer wraps inside a record that will not render and appends the
recovery host instead; the HTML line scan was bypassed by blank-line HTML blocks
and non-fence backtick lines, so it is replaced by the renderer-parse check
above; and a deferred preview could not register a pending edit's editor, so a
file whose session holds one renders its preview at once. The corpus limitation
(P2) stands.

The fourth pass found three P1 remainders, each reproduced and fixed: a remark
container beginning exactly at the region (a list whose first item is edited)
escaped the renderer check, so nodes reaching past the region's end are now
compared and recursed too; remark cannot see rehype-raw nesting an unclosed
`<details>` around the following paragraphs, so the HTML tag-balance rule above
refuses edits that change what stays open or closed; and a file growing past
the preview deferral while an editor was open unmounted that editor and let
Save pass, so Preview stays rendered while a pending edit exists, a missing
editor handle with pending session text refuses, and Source shows a notice
with a way back to Preview. The corpus limitation (P2) stands.

The fifth pass found the tag-balance scan still bypassed by `<details/>`,
quoted `<` in attributes, `<!-->` comments and code edge cases (P1), and
rejecting valid edits inside closed fences (P2). Both are replaced by the HTML
tree check above, which runs the renderer's own HTML parsing instead of
approximating it. The corpus limitation (P2) stands.

The sixth pass found the HTML tree check dropping the descendants of elements
the parser invents without positions (an implicit `tbody`, hiding an unclosed
`<details>` inside a table cell) and its `<` gate skipping a region enclosed
by an open raw element (a Markdown table inside a raw `<table>`), both P1 and
both fixed as described above, plus a P2 on reparsing the committed document:
the cache now keeps two documents per tree kind. Moving validation off the UI
thread for very large documents is noted, not done. The corpus limitation (P2)
stands.

The seventh pass found that an element left open keeps a range covering only
its opener in the HTML tree, so its adopted descendants looked like region
content and went unchecked (P1), and that cache hits were not promoted, so two
refused candidates could evict the baseline (P2). A node now covers its
descendants' ranges, which the enclosure gate inherits, and hits move to the
front. The corpus limitation (P2) stands.

The eighth pass found positionless text the HTML parser re-creates (bare text
after a dropped `<tr><td>`) escaping the check (P1) and the derived coverage
refusing the deletion of an open element's last child (P2). Significant text
now takes part, positionless text placed after what preceded it, and nodes
before the region pair by start so an ancestor may shrink. The corpus
limitation (P2) stands.

The ninth pass showed that placing re-created text by a synthetic cursor is
fragile both ways: it can fall inside the region (P1) or shift with harmless
trailing whitespace (P2). Such text is no longer given an offset at all; it is
checked by its ancestors as described above. The corpus limitation (P2)
stands.

The tenth pass showed that setting aside re-created text by whether its value
occurs in the region's raw text is unsound both ways (P1), and that the HTML
root cache did not promote hits (P2). Provenance now comes from rendering the
document without the region, as described above, and roots are one promoted
cache. The corpus limitation (P2) stands.

The eleventh pass broke the blank-document attribution twice more (merged
outside text; a copy of the outside text inserted in the region), confirming
that value-based attribution cannot be made sound. Provenance is now planted
as canaries, as described above. The added render per Done (P2) is accepted
and documented. The corpus limitation (P2) stands.

The twelfth pass found the canaries themselves perturbing a `$$$` math fence
and a table's delimiter row, and matching omissions (canaries inside
unfinished attributes) passing as preservation; placement, coverage and
ancestor identity are now as described above. The corpus limitation (P2)
stands.

The thirteenth pass found two elements of one tag created inside the region
indistinguishable (P1), coverage refusing ordinary documents with reference
definitions (P1), and canaries landing in an empty fence's info string or a
table's phantom cell after a doubled backslash (P2); identity, placement and
the definition exemption are now as described above. The corpus limitation
(P2) stands.

The fourteenth pass found three more ways the canary proof leaked (reordered
same-tag elements in the region, a reference-definition exemption covering a
block with visible HTML, a canary after a closing tag), with two placement
P2s. Rather than extend a proof that each pass had broken, the commit now
refuses what could change the untouched blocks' rendering, as described in
the Operation section; the rendered-HTML pipeline, the canaries and their
dependency are gone. The corpus limitation (P2) stands.

The fifteenth pass showed six more divergences between the refusal policy's
tag scan and the browser's tokenizer (misnesting, `--!>`, script states,
`<xmp>`, unfinished quotes, attribute serialization). The policy is now
whitelist-based, as described in the Operation section: only self-contained
raw HTML is edited in Preview, blocks with other HTML get no pencil, and in
an unknown HTML context the number of tables may not change. This is the
narrowing the owner may wish to revisit. The corpus limitation (P2) stands.

The sixteenth pass found the grammar accepting Unicode whitespace in a tag
(part of the name to the HTML tokenizer), `<col>` changing a raw table's rows
despite equal table counts, and quoted attribute values refused for angle
brackets; the grammar now uses HTML's ASCII whitespace, the void list is
`br`, `hr`, `img`, `wbr`, and closed quotes may hold brackets. The corpus
limitation (P2) stands.

The seventeenth pass confirmed those fixes and found the grammar reading a
block's raw HTML as one flat sequence: `<b>` before a Markdown table and `</b>`
inside one of its cells balanced on paper while the HTML parser ignores the
closer in the cell, so the opener wrapped the untouched block after the table
(also through the raw-table context rule, whose table count held). Balance is
now checked per Markdown parent, as the Operation section describes. While
confirming the fix through the renderer, the last-block exemption showed a hole
of its own: with footnote definitions in the document, the renderer's footnotes
section follows the last block, and an unclosed element there swallowed it;
footnotes now count as content following every block. Regressions cover both
through Done, with LF and CRLF, and the pencil rule. The corpus limitation (P2)
stands.

The eighteenth pass was cut short by the provider's content filter before a
verdict; its log showed the case it had found: in a raw-table context, a
Markdown table replaced by a footnote definition holding a table kept the
table count while the renderer placed that table in the footnotes section (or
nowhere, unreferenced), so the raw table stayed open and the untouched row
after it changed shape. A differential probe of my own (every accepted edit
from a matrix of context documents and edits, rendered through remark-gfm,
remark-rehype, rehype-raw and rehype-sanitize, comparing the ancestry of every
untouched token) then found the rest of that class: a link, autolink, footnote
reference or raw `<a>` closes a raw link left open; a top-level heading pops a
raw heading left open; a task checkbox closes a raw select; and after an
unclosed `<pre>` or `<textarea>` remark reads the rest of the file as HTML, so
the standalone reading of the edited text saw no HTML at all. The guard now
reads html groups in the whole document (a block is safe when the elements
open before its HTML are open after it) and, in an unknown context, holds the
counts of tables, links (apart inside table cells), headings (apart below the
top level) and checkboxes, in place and per rendered footnote, for the tag
names the earlier HTML mentions, as the Operation section describes. The
probe runs clean over 6,453 accepted edits; a reduced form of it is in the
unit tests. The corpus limitation (P2) stands.

The rerun eighteenth pass confirmed those fixes and found two more: an open
raw paragraph is closed by the next top-level block, so deleting that block
(or replacing it with a definition that renders nowhere) left the inline HTML
block after it inside the paragraph; and the footnotes section renders at the
end of the document, so a table in a newly referenced footnote closed a raw
table left open after the edited block although the block's own context was
known. Top-level blocks are now a sensitivity for an open paragraph or foreign
content, and the section is judged under the state at the end of the
document. Since that paragraph rule would freeze the number of blocks in every
README with a `<p align="center">` header, the grammar now also reads simple
block elements nested strictly (paragraphs, divisions, details, sections,
lists, headings), whose start and end tags the parser handles as plainly as
inline ones, so a closed header leaves the context known; table, select,
raw-text and foreign elements stay unread. A probe of three unclosed raw
formatting elements showed the parser keeps them on its stack, so generated
emphasis or code changes no untouched ancestry and needs no rule. The enlarged
probe (7,833 accepted edits) then showed two limits of the wider grammar: a raw
`<li>` opener closes an open raw item through any inline element, so list
elements stay unread; and a link in a last block reaches back through the
adoption agency algorithm to re-parent a raw division holding earlier blocks,
so the link sensitivity holds for a last block too. Raw heading and
paragraph-closing openers in the edited HTML now count with the generated
ones, and a heading inside a raw element the grammar reads counts apart from a
bare one.

The nineteenth pass confirmed those fixes and found three more. The paragraph
rule counted only a few raw openers, while every admitted block opener and
`<hr>` close an open paragraph: the raw sets now follow the HTML parser's
in-body list of paragraph closers and its list of tags that pop foreign
content. The raw-opener counts used regular expressions, which a comment or
an attribute value such as `<!-- <a> -->` or `alt="<a>"` fooled: raw tags are
now counted as the grammar tokenizes them, and a last block judged for links
must be readable in full. And in a raw table context the parser holds text
until a token follows it, dropping it at the end of input, so deleting the
last rendered block (or replacing it with a definition that renders nowhere)
lost untouched text: under the table sensitivity the signature now records
whether anything still renders from the block on, and that sensitivity holds
for a last block too. The in-place signature now compares the edited region's
contribution as booleans (the first table, link, bare heading, checkbox or
paragraph closer is what acts), so replacing `<details><summary>` by `<p>`
under an open paragraph is fine while deleting it is not; a last block judged
for links must tokenize in full, not balance, so a closing `</div>` block can
still be removed. The enlarged probe (11,247 accepted edits) runs clean.

The twentieth pass confirmed those fixes and found three more. Whether a
heading pops an open raw heading depends on what the parser has open at that
moment, after closing a paragraph implicitly and through raw wrappers, which
the bare/wrapped classes did not model (`<div><h2>` replacing `<h2>`, or
`<div>` around a generated heading after `<h1><p>`): under the heading
sensitivity the region's top-level structure (node types and raw tags as
tokenized) must now hold instead of any count. An unfinished comment counted
as a token that would flush pending table text; raw HTML now counts toward
that only when the grammar reads a complete token in it. And a reference
inside an unreferenced footnote definition made the section look rendered;
footnote order is now derived as the renderer derives it, from references
reached in the document's own content and then in the listed footnotes.

The twenty-first pass approved (VERDICT: APPROVE), after confirming the three
fixes and a further 62,765 accepted Preview-eligible differential probes, with
one nonblocking P2: the structure held under the heading sensitivity recorded
only top-level node types, so an inline raw `<h2>` added inside a paragraph
after an open raw heading escaped it; Codex confirmed no pencil reaches that
block. The structure now records node types at every depth with raw tags as
tokenized, with a regression. The corpus limitation (P2) stands.


## Rich editing in Preview — 2026-10-29

Supersedes the per-block source editor above for Markdown Preview. Governing
decision: the owner's [rich-editing decision](../../chat_intent/rich-text-editing/request.md#human-decision--rich-editing-in-preview--2026-10-29),
which reverses the 2026-10-24 source-only decision and relaxes byte locality.
Research evidence: a public round-trip benchmark found every serializing editor
(Milkdown, Tiptap with its Markdown extension, prosemirror-markdown, remark) rewrote
all six test files on a no-edit save, while splicing only edited ranges kept them
byte-identical; GitLab's Tiptap editor had the same whole-file regeneration and
added a "preserve unchanged Markdown" layer over source positions
([issue 338338](https://gitlab.com/gitlab-org/gitlab/-/issues/338338),
[epic 7256](https://gitlab.com/groups/gitlab-org/-/epics/7256)); GitLab's editor
code is MIT-licensed and built from `@tiptap/*`, ProseMirror, prosemirror-markdown,
lowlight, KaTeX and Mermaid, with no paid packages.

### Governing decisions

- [`g5.documents.preview.rich-editing`](../../ontology/features/documents.g5.md#g5.documents.preview.rich-editing)
  (new): Rich text edits rendered Markdown as a structured document beside a
  read-only Preview; what the editor does not model stays opaque and saves
  verbatim; Monaco remains the engine for code, Source and Compare.
- [`g5.documents.editor.modes`](../../ontology/features/documents.g5.md#g5.documents.editor.modes),
  [`g5.documents.editor.engine`](../../ontology/features/documents.g5.md#g5.documents.editor.engine),
  [`g5.documents.markdown.shared`](../../ontology/features/documents.g5.md#g5.documents.markdown.shared),
  [`g5.documents.source-editing`](../../ontology/features/documents.g5.md#g5.documents.source-editing),
  [`g5.documents.ontology.presentation`](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation)
  and [`g5.documents.ontology.creation`](../../ontology/features/documents.g5.md#g5.documents.ontology.creation)
  are amended: Markdown has a Rich text view while editing, its serialization is
  the one rendered-view write besides domain actions, and statement prose (never
  structure) edits there.
- `g5.documents.preview.block-editing` and `.commit` are retired; their eligibility
  rules survive as the opaque-node rules below.

### Ownership

- **`RichMarkdownEditor`** (`frontend/src/components/files/RichMarkdownEditor.tsx`)
  owns the ProseMirror document, keyboard and toolbar commands, table controls,
  the link editor and node views. It is a view in the sense of [`editor.ownership`](../../ontology/features/documents.g5.md#g5.documents.editor.ownership):
  it holds cursor, selection and in-progress typing, and writes the file's text to
  the shared draft; it never owns a second copy of the file beyond its document.
- **`richExtensions.ts`** owns the schema both sides share: the modeled nodes and
  marks, opaque block and inline nodes, soft breaks, serialization bookkeeping
  (block identity, list looseness, code info strings, table alignment) and the
  plugin that refuses edits removing or duplicating protected record lines.
- **`markdownDocument.ts`** owns the two conversions: Markdown to editor document
  (`markdownToDoc`) and document to Markdown (`docToMarkdown`), including which
  nodes are opaque, the style the file uses, line endings and block preservation.
- **`FileView`** keeps the role gate, Save, Push review, Discard, conflicts and view
  selection; it decides when Rich text shows (Markdown, editing, the Rich text view,
  a person who may edit or suggest) and routes the editor's writes through the same
  guarded draft write every Preview writer uses. `CodeEditor`'s `onBeforeSave`,
  `pendingEdit` and `saveRequest` let Save write Rich text's edits first, stay
  available while they are not yet in the buffer, and run from Rich text's Mod+S;
  nothing in the draft store changes.
- **`DocumentPreview`** returns to a reading view: records, question cards, review
  cards, comments, filters, Compare and the outline stay there and are not shown in
  the editor. It reports a double-click on a block (and the word under it) so
  FileView opens Rich text there. Readers who may not edit see only this view.

### Operation

1. **Entry and views.** For a person who may edit or suggest, Edit (or Suggest
   changes) starts editing in the view the person reads: Rich text from a rendered
   Markdown Preview; Source from Source, from HTML and JSONL previews, and from a
   large file whose preview is deferred. The `e` key (once the document has focus)
   and a double-click on a block in Preview open Rich text too. Rich text opens
   focused: after the double-clicked word, at the end of a block double-clicked
   beside its text, on an opaque block as a whole, or at the start for Edit and
   `e`. While editing the tabs are Rich text, Source and Preview: Source is Monaco
   over the same draft, and Preview stays the read-only reading view of the draft,
   with records, review cards, comments, filters and Compare; double-click or `e`
   there returns to Rich text. Done leaves editing as before.
2. **Load.** `markdownToDoc` parses with the shared remark pipeline (GFM, math) and
   `documentMarkup` for ontology structure, then maps top-level mdast nodes to editor
   nodes: paragraph, heading, bullet and ordered lists with task items, blockquote,
   fenced code, table, thematic break; inline text with bold, italic, strikethrough,
   code, links and hard breaks. A soft line break becomes a node that reads as a
   space and saves as the source's line break, so hard-wrapped paragraphs keep their
   wrapping. Each top-level node records its original source, its indentation (up
   to three spaces) and the whitespace before it; once the editor has normalized
   the loaded document, the baseline seals each node as loaded, and unchanged means
   equal to it. A legacy record label line is split from its body: the label stays
   protected and the body edits as a paragraph that rejoins it on save.
3. **Opaque nodes.** Everything else becomes an opaque block or inline node that
   renders read-only and saves its source verbatim: front matter, raw HTML, math,
   footnote definitions and references, link and image reference definitions and
   reference-style links and images, GitHub alerts and `:::` admonitions, question
   and question-reference fences, lists mixing tasks and plain items or numbering
   tasks, and in ontology files the record identity lines, the anchor, heading and
   metadata lines `documentMarkup` hides, and the `## Metadata` footer. A paragraph
   that reads like a record line is opaque even where the record parser did not
   claim it. Images are opaque inline nodes in this version (no upload, no alt
   editing). Inline opaque nodes keep their exact source as well, written
   unchanged when the paragraph around them is edited; an opaque node can be
   deleted or moved whole like any other node, except the protected lines. Opaque blocks render through the shared `MarkdownPreview` so they look
   as they do in reading view; hidden ontology lines and front matter, which render
   to nothing, show their source instead so they neither vanish nor invite edits. Each
   offers Edit in Source, which opens Source at its line.
4. **Edit.** Tiptap's StarterKit, list, table and link extensions supply the
   behavior: Cmd/Ctrl+B, I, Shift+S for bold, italic, strikethrough, Cmd/Ctrl+E for
   code, Cmd/Ctrl+K for the link editor, Tab and Shift+Tab between table cells and
   list levels, Enter for a new block, Markdown input rules (`# `, `- `, `1. `,
   `[ ] `, `> `, fences), undo and redo. A compact toolbar offers the same plus
   paragraph and heading levels, lists, quote, code block, rule and Insert table;
   inside a table it adds row and column insertion and deletion and table deletion
   (GFM has exactly one header row, so there is no header toggle). A checkbox on a
   task item toggles its state. A table cell holds one paragraph; a line break in
   it saves as `<br>`.
5. **Write.** Debounced after typing (300 ms), on blur, and synchronously before
   Save, Push review, Done, citations, comments and leaving Rich text,
   `docToMarkdown` produces the file text and the editor writes it through
   `FileView.rewriteDraft` with the text it last wrote as the expected content. A
   refused write (the draft changed meanwhile: a Compare merge, a discard, a person
   change) reloads the editor from the draft rather than overwriting it. Edits not
   yet written keep Save and Push review available, and Save submits the shared
   draft's text whenever it is ahead of the Monaco buffer (Rich text writes on blur
   when Save is pressed, before the buffer adopts it). Mod+S in Rich text asks for
   the ordinary Save, whose pre-save hook writes first; for a reader it only
   writes. Save and Push then proceed exactly as for Source edits; the retained
   Monaco model adopts each write as one edit, so Source undo steps are
   write-sized.
6. **Serialization.** A top-level node equal to its sealed loaded node is written as
   its original source; an opaque node always is. Changed or new nodes go through
   `mdast-util-to-markdown` with GFM and math, with the style inferred from the
   file: bullet character, ordered delimiter and numbering, emphasis and strong
   markers, ATX or setext headings, fence character, rule, and table pipe alignment
   and cell padding; list items indent by one space after the marker. An edited
   block keeps its indentation, and a new block takes its neighbour's. Between two
   nodes that were adjacent at load the original whitespace is kept where both
   are unchanged, the body of a split label rejoins it, and a blank line stays a
   blank line; elsewhere one blank line separates blocks. Two neighbouring lists of
   the same kind get different markers so they stay two lists, and whitespace at
   the edge of bold, italic or strikethrough moves outside the mark rather than
   becoming an entity. The file's dominant line ending, its byte-order mark and its
   final newline (or its absence) are restored. This is the "preserve unchanged"
   default the owner relaxed: it is no longer a guarantee, and the design does not
   depend on it; it is kept because readers' suggestions are diffs and agents edit
   the same files.
7. **Bounds.** Rich text sits behind the same 128 Ki deferral as the rendered
   preview: above it, Edit opens Source, and the Rich text tab shows Load preview
   until the person asks for it. Reader suggestion limits apply to the written text
   as before.

### Choices made without the owner

- Rich text is its own editing view rather than replacing Preview while editing.
  The first implementation made the editing-mode Preview tab editable; the CI
  flows then showed what that removed for anyone mid-edit: ontology Compare,
  statement frames, question and review cards, and the read-only rendering several
  journeys assert. Keeping Preview read-only beside Rich text (GitHub's Write and
  Preview, GitLab's rich and plain text) preserves all of it; double-click and `e`
  make entry quick from either reading view.
- Edit starts in the view being read, so a person reading Source keeps editing
  source; Rich text is one tab away.
- Block-level rather than node-level preservation; style inference rather than
  per-node source for edited blocks.
- Images, inline raw HTML and reference-style links are opaque in this version.
- No image upload, merged cells (GFM cannot express them), diagram editors or
  collaboration; Mermaid sources edit as code blocks.
- Tiptap 3.32 (`@tiptap/core`, `react`, `pm`, `starter-kit`, `extension-code`,
  `extension-list`, `extension-table`) and `mdast-util-to-markdown` 2.2 with
  `mdast-util-gfm` 3.1 and `mdast-util-math` 3.0 are the new dependencies;
  GitLab's serializer code was read for shape, not copied.

### Verification

- Unit (`markdownDocument.test.ts`): on the nine-document corpus a no-edit round
  trip returns identical bytes, editing one block changes only that block's lines,
  and an edited document reloads to the same meaning; targeted cases cover CRLF, a
  byte-order mark with front matter, a missing final newline, every opaque
  construct, inline opaque content, style inference, whitespace at mark edges,
  linked code, table cells, insertion, list adjacency, task lists, soft breaks,
  indentation, label splitting, ontology protection and opening at an offset.
- Repository-wide check (a one-off probe over all 893 Markdown files in the
  repository, not committed): a no-edit save is byte-identical for every file, and
  5,692 single-block edits reload to the intended meaning. The only exceptions are
  three deliberately malformed `tests/fixtures/ontology-comparison/refused-*`
  files, where an edit changes how the record parser classifies their structure.
- Component (`RichMarkdownEditor.test.tsx`, `FileView.test.tsx`,
  `CodeEditorFilesOptIns.test.tsx`): Edit opens Rich text for an owner and a reader
  and not for others; typing writes the draft; Save submits the written text,
  including a draft written just before Save that the buffer has not shown; a
  reader's Push carries it; Done returns to reading; Discard drops it; Source
  shows the written Markdown and Preview the read-only draft; double-click and `e`
  open Rich text; a refused write reloads; opaque blocks are not editable and open
  Source at their line; record lines refuse deletion.
- Browser (`file-editor.spec.ts`): paragraph edit with Cmd+B and a `# ` heading,
  exact Save body with CRLF kept, Undo in Source; a code block from the toolbar
  taking typed lines (the shared GitHub stylesheet hides breaks inside code, which
  collapsed an empty editor code block until the editor showed its trailing
  break); a table cell edited in place and
  saved with Mod+S in the file's alignment; double-click on a word, beside a
  block's text and from the read-only Preview; `e`; a reader's Push including
  unwritten edits; a person change dropping written and pending edits; Save over an
  external change taking the conflict path; ontology prose edited while record
  lines refuse deletion and Compare stays in Preview; a deferred large file opening
  Source. `markdown-layout.spec.ts`: Rich text fills the Markdown surface's content
  box with the reading view's padding at 1800 and 900 px, without overflow. The CI
  flows that type source after Edit select the Source tab.
- The existing FileView, DocumentPreview, CodeEditor and page suites pass with the
  block-editing cases replaced.
- Measured once on the Vite dev server (not committed) with a 125,178-character
  document just below the deferral threshold (398 sections of headings, formatted
  paragraphs, lists, tables and code): the reading Preview rendered in 2.4 s with a
  longest main-thread task of 1.0 s; Rich text opened in 2.0–2.5 s with a longest
  task of 1.1 s; typing took about 18 ms a key, each debounced write blocked for up
  to about 130 ms, Save took 0.4 s, and the saved text was the file plus exactly
  the typed characters.

### Self-review

The design keeps one authoritative editing session (the draft), one write path
(`rewriteDraft`), one Save slot and the existing conflict and suggestion rules; it
adds no store. It reverses two recorded decisions deliberately, with the owner's new
decision as authority, and narrows "rendering never rewrites source" to Rich text's
explicit writes. Content loss is prevented by construction (opaque nodes and the
record-line guard) rather than by serializer fidelity; formatting churn is bounded
by block-level preservation and style inference, as a default rather than a rule.
Browser journeys found three defects the component tests could not: Save pressed
right after typing saw a buffer that had not adopted Rich text's blur write; `e`
never reached FileView because clicking the rendered document left focus on the
page; a double-click beside a block's text placed the caret at its start. Each is
fixed and covered. Open assumptions: the opaque set is complete for the corpus
(verified across the repository's Markdown, extended as found); opening Rich text
near the threshold costs about what rendering the reading Preview does, so the
existing deferral bounds both, though a debounced write on such a document still
blocks for about a tenth of a second.

## One surface with a rich/plain text switch — 2026-10-30

Governing decision: the owner's
[one-surface decision](../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30).
It supersedes the agent choice above that kept Rich text as its own tab beside a
read-only Preview and Source, settles the deferred Preview-tab question, makes Done
always available, and limits editor text to 8 inches.

### Governing decisions

- [`g5.documents.editor.modes`](../../ontology/features/documents.g5.md#g5.documents.editor.modes)
  (amended): a Markdown file is one surface in two modes, rich text and plain text,
  switched at the foot of the view; no mode tabs; HTML and JSONL keep their
  previews as the rendered mode of the same switch; other files are plain text only.
- [`g5.documents.preview.rich-editing`](../../ontology/features/documents.g5.md#g5.documents.preview.rich-editing)
  (amended): Rich text opens in place of the reading view, plain text shows the
  same draft, and Done always returns to the reading view with the draft applied
  while Save, Push review and Discard stay until the person decides.
- [`g5.documents.text-measure`](../../ontology/features/documents.g5.md#g5.documents.text-measure)
  (new): the 8-inch column and wrap, the HTML/JSONL exemption, the table default.
- [`g5.documents.editor.engine`](../../ontology/features/documents.g5.md#g5.documents.editor.engine),
  [`g5.documents.source-editing`](../../ontology/features/documents.g5.md#g5.documents.source-editing),
  [`g5.documents.files-view.drafts`](../../ontology/features/documents.g5.md#g5.documents.files-view.drafts),
  [`g5.documents.ontology.presentation`](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation)
  and [`g5.documents.ontology.creation`](../../ontology/features/documents.g5.md#g5.documents.ontology.creation)
  are reworded from Source/Preview views to the two modes; plain text wraps at the
  measure; reopening retains the mode and whether the person was editing.

### Ownership

- **`FileView`** keeps the role gate, Save, Push review, Discard, conflicts and
  entry. Its state shrinks to `editing` and the mode (`view`: rendered `preview`
  or `source`); the former `rich` value is gone, since rendered-while-editing is
  Rich text whenever the person may edit. Both facts are retained per file session
  in `fileDrafts` (`saveFileViewMode`, new `saveFileEditing`), so a reopened file
  comes back in the same mode and the same reading/editing state. FileView renders
  the switch into the editor's status bar and decides its label; its header shows
  Edit or Suggest changes while reading, Done while editing, and Discard and Push
  review whenever a draft exists.
- **`CodeEditor`** gains three small FILES hooks: `statusBarAction` (a slot at the
  right of the status bar), `saveAvailable` (Save is offered and enabled on a
  read-only buffer that mirrors an undecided draft, so the owner can Save from the
  reading view after Done), and the plain text wrap: Monaco runs with
  `wordWrap: "bounded"` at the column count its measured font gives for 768 px,
  re-measured when the font changes, except for HTML and JSONL
  (`textMeasure.ts` owns the measure and the exemption). The draft mirror, the
  Save slot and conflict handling are unchanged.
- **`DocumentPreview`** and **`RichMarkdownEditor`** keep their behaviour; their
  document column and rich surface share the `g5-doc-column` rule, so rendered
  text and edited text sit in the same centred 8-inch column. Opaque nodes say
  "Edit in plain text".
- **`fileDrafts`** retains one more fact per session; there is no new store and
  no new write path.

### Operation

1. **Open.** Markdown opens rendered, in reading; other text opens in read-only
   plain text; HTML and JSONL open in their previews; a line citation opens plain
   text at the line, as before.
2. **Edit.** Edit, Suggest changes, `e` and a double-click start editing in the
   mode being read: Rich text from rendered Markdown (at the double-clicked block),
   plain text from plain text, from an HTML or JSONL preview, and from a deferred
   large preview.
3. **Switch.** The status-bar button toggles the mode in reading and editing
   alike, flushing Rich text first; a refused flush keeps the mode, as the tabs
   did. Labels: while editing Markdown, "Switch to plain text editing" and
   "Switch to rich text editing"; while reading, "Switch to plain text" and
   "Switch to rich text"; for HTML and JSONL the way back reads "Switch to
   preview". Files with no rendered form have no switch.
4. **Done.** Always shown while editing. It flushes Rich text, ends editing and
   shows the rendered reading view (plain text for files without one) with the
   draft's text. The draft stays: the editor's "Unsaved · browser only" mark,
   Save (owner), Push review (reader) and Discard remain in the reading view
   until Save, Push or Discard settles it; Edit resumes editing in the current
   mode. The read-only plain text of a file with a draft shows the draft too.
   Discard holds its place throughout editing (disabled until a draft exists):
   Rich text writes its draft when a header button takes focus on mousedown,
   and a button appearing then shifted Done and Push review from under the
   pointer before the click landed, which the browser journeys caught.
5. **Domain writes.** A question answer or HTML answer from the reading view
   writes the draft as before but no longer flips the view into editing: the
   draft's presence shows the decision controls, so the person stays on the card
   they were answering.
6. **Width.** `--g5-text-measure: 8in`. `g5-doc-column` caps the document column
   and the rich surface at the measure plus their 24 px side padding and centres
   them in the pane. Monaco wraps at `floor(768 / typicalHalfwidthCharacterWidth)`
   columns, bounded by the viewport, for every language except `html` and `jsonl`
   (by language or extension). Tables keep GitHub's rule: cell text wraps, and a
   table whose minimum width still exceeds the column scrolls inside it; the rich
   editor's table wrapper gets the same overflow. Compare is untouched.

### Choices made without the owner

- The switch exists while reading too, with "editing" dropped from its labels:
  line citations, comment placement and the raw Markdown need plain text before
  Edit, and a reader who cannot edit must not be offered "editing".
- HTML and JSONL keep their previews on the same switch rather than becoming
  plain-text-only files; the owner's "only source mode" is read as describing
  files without a rendered form. Their editing stays plain text.
- The column is centred, not left-aligned, matching Docs-style readers; plain
  text stays left-aligned with its gutter, so the two modes differ in alignment.
- Whether the person was editing is retained with the mode, so Done followed by
  navigation does not reopen the editor, and Edit followed by navigation does not
  silently close it.
- Compare keeps its side-by-side diff without wrapping; chat and other Markdown
  surfaces outside the file view are not changed.

### Wide tables — recommendation for the owner

The implemented default is GitHub's: a table first wraps its cell text to fit the
column; only when the longest unbreakable word or number in each column still
needs more than 8 inches does the table scroll horizontally inside the column, in
reading and in Rich text alike. Nothing is clipped and the page never scrolls
sideways. The alternatives, in order of preference:

1. **Keep the default** (implemented). Predictable, no per-block layout rules,
   same behaviour as GitHub and GitLab. Cost: a very wide table is read through a
   scrollbar even on a wide screen.
2. **Let tables bleed.** A table wider than the column may grow up to the pane
   width, centred over the column, before it scrolls. Wide screens show more of
   the table; the column stays 8 inches for prose. Cost: a layout rule for one
   block type in both the reading view and Rich text, and the outline and
   comments rail reduce the room it can take.
3. **Plain text cannot help.** Monaco wraps lines, not table cells, and wraps the
   whole file or nothing; a wrapped table row is unreadable. If the owner wants
   wide tables legible in plain text, the choice is to exempt Markdown tables
   from wrapping by switching the whole file's wrap off while the caret is in a
   table, which is surprising. The recommendation is to edit wide tables in Rich
   text, where cells wrap and the table scrolls.

### Verification

- Unit (`FileView.test.tsx`): Markdown opens rendered with the switch reading
  "Switch to plain text"; the switch toggles read-only plain text and back; Edit
  opens Rich text and the switch reads "Switch to plain text editing", then
  plain text editing shows the written Markdown and "Switch to rich text
  editing"; Done always shows while editing and returns to the reading view with
  the draft applied, where Save and Discard remain and Save submits the draft;
  Done then Discard drops it; a reader's Done keeps Push review; a question
  answer from the reading view does not enter editing; HTML keeps its preview
  with "Switch to plain text" and "Switch to preview"; a file without a rendered
  form has no switch; a line target opens plain text with "Switch to rich text";
  reopening after Done stays in reading, and reopening while editing resumes.
  `fileDrafts.test.ts` covers the retained editing flag. `textMeasure.test.ts`
  covers the exemption and the column count.
- Page tests (`WorkspacePageComments`, `WorkspacePageFileLinks`,
  `WorkspacePageV2`) select plain text through the switch.
- Browser (`file-editor.spec.ts`): the former tab clicks become switch clicks;
  Done after a Rich text edit shows the reading view with the edit, Save still
  offered, and Save submits exactly the edited bytes; plain text wraps at the
  measure (`wordWrap` bounded at the measured column, a long line taking several
  view lines) and an HTML file does not; a wide table scrolls inside the column
  without widening the page. `markdown-layout.spec.ts`: the document column and
  Rich text are 8 inches plus padding at 1800 px, fill the pane at 900 px, and
  Rich text's text keeps the reading view's x position. `project-doc.spec.ts`
  and the CI flows (`files-source-editor`, `explicit-save`, `agent-file-links`,
  `document-independent-suggestions`, `helpers.openFileSource`) select plain text
  through the switch; they run against a real backend in CI only.

### Self-review

The change removes state (the `rich` view value and the editing-mode tab set)
and adds one retained boolean; the draft remains the single authority for
unsaved text, and Save, Push review and Discard keep their one path each. Done
keeping the draft is safe because the reading view already renders the draft's
text and CodeEditor's mirror keeps the buffer on the draft while the view is
read-only; Save from the reading view goes through the same `handleSave` with
`saveAvailable` only widening the button's gate, never `persist`'s session rules
beyond `editable`. Domain writes no longer entering editing is a behaviour
change for question cards, covered by a test, and consistent with the owner's
"Save/Discard stay visible until you decide". The 8-inch rule narrows the
2026-10-27 "fit-width" layout evidence deliberately, by the owner's decision,
and the layout spec is rewritten rather than loosened. The browser journeys
found one defect the component tests could not: with Done always present, the
Discard button that appeared on Rich text's focus-loss write moved Done and
Push review under the pointer, so a click right after typing was lost; the
header now reserves Discard's place while editing. Open: the wide-table
alternative above; whether the owner wants the column left-aligned.

### Owner follow-up — tables exempt, controls in the file row — 2026-10-30

The owner's [second decision of the day](../../chat_intent/rich-text-editing/request.md#human-decision--tables-exempt-from-the-measure-controls-in-the-file-row--2026-10-30)
takes the "let tables bleed" alternative above and moves every control to the
right of the file name row.

- **Tables.** The measure moves from the document column to each top-level
  block: `g5-doc-column` now fills the pane, and every direct child of the
  Markdown body except a table (or Rich text's table wrapper) is centred at
  the measure. A table has `min-width: min(measure, 100%)`, so a small one
  keeps the text's left edge, and `width: max-content; max-width: 100%`, so a
  wide one grows to the pane and GitHub's table rule scrolls it inside. Measured
  before the decision at 1800 px: prose 768 px, a 14-column table 1720 px in
  reading and 1752 px in Rich text, scrolling inside, no page overflow; at 700
  px everything fills the pane. Nested tables stay within their list item or
  statement frame; plain text cannot exempt table rows.
- **One file row.** `CodeEditor` renders a single row: the path, the source
  badge and "Unsaved · browser only" at the left, then at the right whatever
  the caller passes through `headerActions(save)` (FileView: role badge, mode
  switch, Edit or Suggest changes or Start, Push review, the editor's Save,
  Done, Discard) and the Comments toggle. FileView's own header row, the
  editor's path row and its toolbar (with the redundant language label, which
  the status row already shows) are gone; FileView's notices and push panel
  render under the row through `notices`. The status row keeps line, column,
  language and the minimap toggle. Discard still holds its place while editing
  for the reason recorded above.
- **Verification.** `markdown-layout.spec.ts` now asserts the column fills the
  pane while prose is 768 px and centred, in reading and Rich text alike;
  `file-editor.spec.ts` asserts a small table shares the text's left edge and a
  wide one exceeds the measure, stays within the pane and scrolls inside, in
  both views; FileView tests assert the controls sit in the editor's file row
  and the source badge is named there in both modes.
