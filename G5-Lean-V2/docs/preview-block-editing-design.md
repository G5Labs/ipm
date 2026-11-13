# Edit Markdown blocks in Preview

Status: design, approved direction by the owner on 2026-10-26. **Superseded on
2026-10-29:** the owner replaced per-block source editing with rich editing of
the whole rendered document (the Rich text view, PR #943; decision in
`.g5/chat_intent/rich-text-editing/request.md`, design in
`.g5/pr_ontology_diff_design/rich-text-editing/ontology-plan.md`). The pencil,
block editor and `blockEdit.ts` described below no longer exist. Kept as the
record of the approved direction and the measurements that informed it.

## Goal

When a Markdown file is open in Preview (from FILES, PROJECT DOCS, CORE ONTOLOGY, a chat citation, a review card or a deep link, which all land on the same `FileView` tab), a person who may edit the file can edit one rendered block at a time without switching to Source. The edit changes only the bytes of that block. Everything else about editing stays as it is: Save commits and pushes for the owner, Push review records suggestions for a reader, conflicts, Compare, Discard and Undo keep working.

Why not a WYSIWYG library: every mainstream rich Markdown editor parses the whole file into its own model and writes the whole file back, which rewrote the file on a save with no edits in 63 of 66 cases measured on 2026-10-03. Editing one block's source text in place avoids that entirely.

## What the person sees

- In Preview, hovering or keyboard-focusing an editable block shows a small Edit (pencil) button at the block's right edge, styled like the existing statement info button. The button is an overlay inside the preview's existing horizontal padding; it reserves no width, so the block's text keeps the full column (see Layout below).
- Activating it replaces the rendered block with a plain-text editor (a `textarea` sized to the block's text, monospace, auto-growing) containing that block's Markdown source, plus Done and Cancel. Escape cancels; Cmd/Ctrl+Enter is Done.
- Done splices the text into the file and re-renders the Preview. The toolbar now shows the editing state that the Edit button normally starts: Save (owner) or Push review (reader), Discard, and the conflict or changed-on-disk notices if they apply. Done with unchanged text does nothing.
- Only one block editor is open at a time. Pressing Save with a block editor open commits it first.
- The selected view does not change. Edit and line citations still open Source. The "Load preview" deferral for large files is unchanged; block editing is available only once the preview is rendered.
- Readers get the same button when they may suggest changes (the existing "Suggest changes" rule). Their first Done starts a suggestion draft; Push review carries it exactly as a Source edit would, including the stale-base warning and the 2 MiB suggestion limit.

## Layout: fit width, as in PR #849

PR #849 ("Fix premature wrapping in shared Markdown previews") makes Markdown fill the pane left after sidebars and padding: the shared renderer gets `width: 100%; max-width: none; min-width: 0; box-sizing: border-box`, its old 980px cap and page padding are gone, and `DocumentPreview` owns the page padding (`px-6 pt-4 pb-8`) on its outer Markdown surface. Its browser spec (`frontend/e2e/browser/markdown-layout.spec.ts`) measures that `document-preview`, `document-preview-content` and the `.g5-markdown-preview` surface inside it have the same width, that a short paragraph stays on one line at 1800px and wraps at 900px, and that nothing overflows horizontally. Block editing must keep all of that true:

- Implement on top of #849 (branch from `main` once it has merged, or rebase onto it). The two change sets touch `DocumentPreview.tsx` and `index.css` but different lines.
- The pencil is `position: absolute` inside the block's `relative` wrapper, placed in the 24px padding gutter at the right. It never adds width, margin or padding to the block, and it never causes horizontal overflow.
- The in-place editor replaces the rendered block element inside the same `.g5-markdown-preview` surface, with `width: 100%; max-width: none; min-width: 0; box-sizing: border-box` and `white-space: pre-wrap`, so it fills the column exactly as the block did and long lines wrap instead of widening the page.
- No new width cap anywhere, and no change to the `document-preview`, `document-preview-content` or `.g5-markdown-preview` structure the spec measures.
- Add one case to that spec: with a block editor open at 1800px and 900px, the editor's box equals the Markdown surface's content box and there is no horizontal overflow.

## Which blocks are editable

Editable: top-level blocks that `parseMarkdownToBlocks` (`frontend/src/lib/utils.ts`) yields with a `sourceRange` and that the preview rendered with a `data-block-index` (set by `frontend/src/components/files/documentMarkup.ts`): paragraphs, headings, lists, block quotes, fenced and indented code, tables, horizontal rules and HTML blocks. In ontology files this includes the prose inside a statement.

Not editable, because the block carries identity or a structured payload that has its own controls:

- front matter (`type: "frontmatter"`);
- current-ontology anchor and marker lines, record heading lines and record metadata (these are already in `markup.hidden`), and a legacy record's label paragraph (`record.spanStart`);
- `g5-question` fences (the question card keeps its own answer controls);
- blocks hidden by the Human, Risk or Unimplemented filters, or while Compare is on;
- blocks covered by an open review suggestion (they keep Accept and Reject);
- any rendered element without a `data-block-index` (the two parsers did not agree on its start offset); it stays editable in Source.

Granularity is the top-level block. A long list or table edits as one block in this version.

## How it works

No new store, no new backend route, no new permission. The commit path is the one question answers already use.

1. **Block source.** For block `i`, the source is `content.slice(start, start + length)` from its `sourceRange`. Ranges exclude the blank lines between blocks, so surrounding whitespace is never touched. If the file uses CRLF (the range includes the `\r`), show the text with `\n` and restore `\r\n` on commit.
2. **Splice.** `next = content.slice(0, start) + edited + content.slice(start + length)`.
3. **Guard.** Re-parse `next` with `parseMarkdownToBlocks` (same `ontology` flag the preview used). Every block before `i` must have the same type and the same source text; every block that starts at or after the new end of the edited region must correspond one-to-one, in order, to the old blocks after `i`, with the same type and source text (offsets shifted). If this fails (typically an unclosed fence or HTML block swallowing what follows), refuse the commit, keep the block editor open and show why.
4. **Write.** Call `FileView`'s existing guarded rewrite with `(content, next)`. Today it is `rewriteOwnerDraft(expected, next)` and is owner-only; generalize it to the viewer's role (`"owner"` or `"suggest"`, the same `mayEdit` gate used by the Edit and Suggest buttons, plus not `pushing` and not `draft.saving`). It already refuses the write when the draft changed meanwhile or a Save is in flight (`writeFileDraft` with `expectedContent`), sets `editing`, and keeps the Preview view.
5. **Model adoption.** `CodeEditor` already mirrors the draft store and adopts a draft changed elsewhere into the Monaco model with `pushStackElement` + `pushEditOperations`, so Undo in Source restores the pre-edit text and the retained history survives. Nothing to change there.
6. **Save and push.** Unchanged. The owner's Save sends the full text with the expected hash and VM id and commits "Save {path}"; the reader's Push review sends suggestion hunks. Comments re-anchor on buffer change as they do for Source edits; an open suggestion on another block goes stale exactly as it would after a Source edit.

### Code touch points

- `frontend/src/components/files/blockEdit.ts` (new): pure functions `blockSourceText(content, block)`, `spliceBlock(content, block, edited)`, `untouchedBlocksPreserved(oldBlocks, newBlocks, editedIndex, newEnd)`, `editableBlockIndices(markup, fences, overlay, filters)`.
- `frontend/src/components/files/DocumentPreview.tsx`: the Edit button per editable rendered element (keyed by `data-block-index`), the in-place editor state (one open block), Done/Cancel/keys, and a new prop `onEditBlock?: (expected: string, next: string) => boolean` gated by a new `canEditBlocks` prop (distinct from the owner-only `canAnswer`).
- `frontend/src/components/files/FileView.tsx`: generalize `rewriteOwnerDraft` to `rewriteDraft(role)` for both roles; pass `canEditBlocks` and `onEditBlock` to `DocumentPreview`; make Save commit an open block editor first.
- `frontend/src/index.css`: button and editor styles, following the statement info button and the existing suggestion-zone layering.

## Acceptance

- Unit (`blockEdit.test.ts`): on the six files used in the 2026-10-03 experiment plus three files from `.g5/ontology`, a Done with unchanged text yields the identical string; a one-word edit changes only that block's bytes; an unclosed fence is refused; CRLF files keep CRLF; a block deleted to empty text leaves the surrounding bytes intact.
- Component (`DocumentPreview.test.tsx`, `FileView.test.tsx`): the button appears only on editable blocks for an eligible person; front matter, record identity lines, question fences, filtered blocks and blocks under an open suggestion have none; a reader's Done creates a `suggest` draft; Done with no change creates no draft; a refused commit keeps the editor open.
- Browser with real Monaco (`frontend/e2e/mock-preview/file-editor.spec.ts`): edit a paragraph in Preview, Save, and assert the PUT body differs from the original only in that block's lines; switch to Source and Undo restores the text; Push review carries a block edit; a person change drops the draft; the Save-conflict path is unchanged.
- All existing `FileView`, `DocumentPreview`, `OntologyFileDrafts` and workspace page suites pass unchanged.

## Out of scope

A rich-text per-block editor (would normalize the edited block); editing below the top-level block (list items, table rows); creating, renaming or deleting files; any change to Source mode, to the Save API or to the backend; chat, comment and issue composers, which do not share the file editor.
