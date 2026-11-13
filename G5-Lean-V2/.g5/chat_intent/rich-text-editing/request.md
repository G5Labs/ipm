# Edit Markdown blocks in Preview — human request

## Human request — 2026-10-27

The owner asked the agent to implement the design at
[`docs/preview-block-editing-design.md`](../../../docs/preview-block-editing-design.md)
("Status: design, approved direction by the owner on 2026-10-26"), following
the design workflow first and then implementing; to have Codex (astra model,
extra-high effort) review the change before filing a PR; and then to drive the
PR to shippable: green checks, P0/P1 and Critical/Major review comments answered
inline, and no merge conflict.

The design decides the product behavior: one rendered top-level block at a time
is editable in Preview as that block's plain Markdown source; Done changes only
that block's bytes; Save, Push review, Discard, Compare, conflicts and Undo stay
as they are; no WYSIWYG library, because whole-document rich editors rewrote the
file on 63 of 66 no-edit saves measured on 2026-10-03. It also fixes the layout
rule: implement on top of PR #849's fit-width Markdown surface and add one case
to its browser spec. The owner's earlier
[source-only decision](../workspace-colab-harden/unified-editor.md#source-only-editing--2026-10-24)
is narrowed by this design for single-block edits; whole-document rich editing
remains out of scope.

## Agent interpretation

Decisions the design leaves to implementation, recorded in the
[plan](../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md):

- PR #849 is open, not merged, so this work is stacked on its last substantive
  commit (`2ecd2f66f`) and will be rebased onto `main` once #849 lands.
- "Pressing Save with a block editor open commits it first" needs a pre-save hook
  on the shared source editor, whose toolbar owns Save; the design's "nothing to
  change there" concerns model adoption, which is indeed unchanged.
- Pending block text is kept with the browser-tab file session, like the selected
  view, so navigation does not lose it; Save, Push review, leaving Preview and
  opening another block commit it first; Discard and a person change drop it.
- The six files of the 2026-10-03 experiment are not in the repository (the
  research report was never tracked), so the unit acceptance runs on six
  repository Markdown files of comparable variety plus three ontology files.
- Ctrl/Cmd+S inside the block editor acts as Done, so the browser's save-page
  dialog never opens; Save remains the toolbar or Monaco action.

## Human decision — rich editing in Preview — 2026-10-29

> the block edit shipped from this branch does not really meet my expectation because when i hit the edit button edit a given block, user still edit in source mode of markdown, means that user still need to remember the markdown syntax, and even worse, user can only edit existing block and cannot create new block
> please research on ways to make it truly edit in preview mode, for example if there is a table, user edit table cells directly when table is in preview mode, user can use keys cmd+B for bold, cmd+I for italic etc..
> we can relax the constraint, i.e, a save must not rewrite what the person did not touch
> ok, please build your recommendation in full, you should validate your change, commit and push but you must not file PR until i explicitly ask you to file

Human decision: Preview editing is rich, not source: table cells edit in place,
Cmd+B and Cmd+I format, new blocks can be created. This supersedes the
[source-only decision of 2026-10-24](../workspace-colab-harden/unified-editor.md#source-only-editing--2026-10-24)
and the per-block source editor shipped by PR #871. The byte-locality rule from the
owner's [block-editing design](../../../docs/preview-block-editing-design.md) (a save
must not rewrite what the person did not touch) is relaxed. No pull request until
the owner asks.

Agent interpretation (research of 2026-10-28 and 2026-10-29, recorded in the
[plan](../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#rich-editing-in-preview--2026-10-29)):
build GitLab's shape, a ProseMirror editor through Tiptap over the whole Markdown
document, fed from the shared remark pipeline, writing Markdown back through a
serializer; keep unchanged top-level blocks verbatim as a default because readers'
suggestions and agents' concurrent edits still suffer from whole-file rewrites;
represent every construct the editor does not model as an opaque node that saves
its source verbatim, so relaxing formatting never loses content; Monaco remains the
engine for code files, Source and Compare. The owner asked how an exact GitLab
match would differ and whether Monaco stays; the answer is in the plan.

## Human decision — one surface with a rich/plain text switch — 2026-10-30

> I do not see a reason why we keep three modes, i.e, `Rich text`, Preview and Source
> please change to
> if it is a markdown file, when user opens it, it is defult in Rich Text mode;
> when user double click, user can edit but still in Rich Text mode;
> at the very bottom of the window, i.e, the same row/place we we show something like "Ln 1, Col 1 markdown UTF-8" today, we add a button named "Switch to plain text editing", if user click it, it basically switchs to the source mode and user can keep editing in source model but the button will change to "Switch to rich text editing", again if user click it, it becomes today's rich text editing mode
> so, we can also remove the row/place at the top that shows `Rich text`, Preview and Source
> Note, With the Preview tab gone, the only way back to the reading view (ontology Compare, record frames, question and review cards) while editing is Done. Today Done is hidden until you Save or Discard. We need to make "Done is always available; it returns to the reading view with your unsaved changes applied, and Save/Discard (or Push review) stay visible there until you decide."
> if it is NOT a markdown file, then there is only one mode, which is soure mode (here I think we should open with monaco, but please correct me if wrong)
> in all cases of editor, no matter if markdown or not, I want to limit the width of text to 8inch, and wraps if the line is wider than 8inch. Note, html and jsonl is an exception here
> and please suggest how should we deal with super wide table.

Human decision: a Markdown file has two modes of one surface, rich text and plain
text, switched by one button at the foot of the view where the line, column and
language status sits ("Switch to plain text editing" / "Switch to rich text
editing"); the mode tabs at the top (Rich text, Preview, Source) go. Markdown opens
in rich text; a double-click edits there. Done is always available while editing
and returns to the reading view with the unsaved changes applied; Save and Discard
(or Push review) stay visible there until the person decides. Files that are not
Markdown have plain text only (Monaco). Every editor limits text to 8 inches and
wraps longer lines; HTML and JSONL are exempt. The owner asked for a recommendation
on very wide tables. This supersedes the 2026-10-29 agent choice of a read-only
Preview tab beside Rich text and settles the deferred Preview-tab question.

Agent interpretation (recorded in the
[plan](../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#one-surface-with-a-richplain-text-switch--2026-10-30)):

- The switch also works while reading, because line citations, comment placement
  and readers who want the raw Markdown still need plain text; its labels then
  drop "editing" ("Switch to plain text" / "Switch to rich text").
- HTML and JSONL keep their specialized previews as the rendered mode of the same
  switch ("Switch to preview" leads back), since the owner's "only source mode"
  describes files with no rendered form; their editing is plain text, as today.
- The 8-inch column is centred in the document pane, and rendered Markdown shares
  it with Rich text so entering editing moves no text; Monaco wraps at the column
  count its font gives for 8 inches. Compare keeps its side-by-side layout.
- Whether the person was editing is retained with the selected mode, so Done
  followed by navigation does not reopen the editor.
- Wide tables: by default a table wraps its cell text and then scrolls inside the
  column; letting tables extend beyond the column is offered as the alternative.

## Human decision — tables exempt from the measure; controls in the file row — 2026-10-30

> looks good to me, please make the exemption for table width
> also can we move button `Switch to plain text`/`Switch to rich text`, Save, Comments, Edit, Dicard etc to the right side of the file name row, i.e, (docs > {uuid}> {filename}.md)?

Human decision: tables are exempt from the 8-inch measure (the agent's
"let tables bleed" alternative, measured before the owner chose it: prose keeps
the measure, a wide table grows to the pane and scrolls inside itself). Every
control of the file view, including the rich/plain text switch that the
morning's decision had put in the status row, moves to the right side of the
file name row (`docs › workspace › file.md`).

Agent interpretation: one row owned by the shared editor replaces three (the
view's header, the editor's path row and its toolbar). Order at the right:
role badge, mode switch, Edit or Suggest changes or Start, Push review, Save,
Done, Discard, Comments; the source badge and "Unsaved · browser only" sit
beside the path. Tables nested in lists or ontology statement frames stay
within their block; a small table stays flush with the text.
