# Core Ontology discovery and format support

## Human request — 2026-10-13

The user reported that Core Ontology says “No ontology nodes” and asked whether
the move to `.g5/ontology` caused it. After a source investigation confirmed the
discovery mismatch, the user asked: “Let's plan how to fix the Core Ontology
sidebar to read the files under .g5/ontology and support the current ontology
format. What are the decisions / open questions we need to make / answer?”

The initial request authorizes planning. The answers below settle its five product
decisions; they do not request implementation.

## Agent interpretation

Support needs to extend through opening the discovered files and preserving their
current Markdown representation. Existing navigation, editor and file-authority
contracts still govern. The selected design and engineering recommendations are in the
[plan](../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md).
No implementation or deployment is requested in this turn.

## Human decisions — 2026-10-13

1. Sidebar: repository → folders → files. Show only repositories containing
   ontology. If exactly one qualifies, show its ontology directly; if several
   qualify, group by repository.
2. Presentation: use the recommendation to retain block editing and source access,
   display the heading, kind and stable ID together, and provide expandable metadata
   details backed by the authored footer as the single metadata source. The
   statement-title presentation is superseded by the 2026-10-14 correction below.
3. Compatibility: no legacy ontology support is needed. Assume manual migration to
   the current format; no compatibility work is required.
4. Relationships:
   - Open relative document and source links inside G5, preserving the repository
     and jumping to the specified heading.
   - Adapt the existing popover to preview a linked ontology block with “Open
     document” navigation. Whole-document links simply open the document.
   - Make footer provenance, evidence and implementation links clickable while
     preserving their authored meaning.
   - Support current-format reference navigation; exclude legacy graph
     compatibility and defer dependency visualization. Ordinary citation or
     evidence links do not necessarily declare dependencies.
   - Retain intent tracing where explicit grounding records exist. Do not infer
     records from stable IDs or footer links, or present unsupported current-format
     files as having “no relationships.”
   - Leave the retired combined resolved-context panel out of this change.
5. Moves: use normal authoring link maintenance. Agents preserve block IDs, move
   their metadata and update affected Markdown links. Refresh the sidebar to show
   the new location and follow updated links in the editor. Stale links/tabs show a
   clear missing-target message. Defer automatic redirects, relocating open tabs and
   transferring comments; automatic move tracking is not a blocking question.

## Human correction — 2026-10-13

“Support current-format reference navigation; exclude legacy graph compatibility
and defer dependency visualization.” This supersedes the earlier instruction to
keep existing legacy graphs. Decision 4 above reflects the corrected scope; the
other reference-navigation and explicit-grounding decisions are unchanged.

## Human correction — 2026-10-14

The user clarified that statement titles will not be shown, questioned whether humans
can move statements, and asked to revise the plan using this selected feedback:

- Exclude statement-title controls and human statement movement from this change.
- Preserve IDs during ordinary human content edits.
- Handle agent-authored moves by rereading files; agents maintain IDs, metadata and
  affected Markdown links.
- Keep metadata handling for supported actions, such as deleting a statement, without
  adding a move feature.

This supersedes the statement-title display/editing, human move and ID-renaming
assumptions in the previous design. Agent interpretation: retain authored headings
and IDs in Markdown for parsing/navigation; do not add an editable raw-source mode
inside Core Ontology. Existing Files source editing remains available. No application
implementation is authorized by this plan-revision request.

## Human creation decision — 2026-10-14

“Let's use your recommendation: leave statement creation to agents/Files and remove
Add I/D/Q from Core Ontology. Update the plan accordingly.”

New statements are authored through agents or the existing Files editor. Core Ontology
retains content editing and supported deletion of existing statements, with no Add
I/D/Q composer. This settles the remaining creation question; application implementation
is not requested by this plan update.

## Shared editor simplification — 2026-10-14

The human stated that the new ontology no longer uses `intent_record/` and
`intent_grounding/`, then authorized: “Remove Source and Drives everywhere in the
shared editor, and hide statement IDs under Metadata. Update the existing plan
and implement.”

This supersedes shared-editor explicit-trace controls and visible statement IDs.
Interpretation: apply to the shared statement frame across ontology, documents and
read-only Markdown consumers; preserve IDs in source and navigation. Keep independent
All intent logs tracing and backend ledger consumers outside this UI change.

## Compact shared statements — 2026-10-14

The human accepted a compact document-like shared statement layout, with these
corrections: put Delete statement in the right-click menu with Shift+F10 access;
use small muted Intent, Design and Question labels beside the first line; do not
change text width. Update the existing plan and implement.

The accepted recommendation removes filled cards, heavy borders and separate large
headers, reduces spacing, moves collapsed Metadata below content, keeps meaningful
status visible, and exposes editing affordances on hover or keyboard focus. Existing
document/group headings and protected authored statement headings remain intact.

## Ben ontology integration — 2026-10-14

The human approved applying all six unique commits on top of the new ontology
structure from ziniuw/ontology_update_based_on_ben, on saehan/ontology-editor in
the same checkout. Preserve the target ontology structure, reconcile governing
commitments, adapt the editor to explicit anchors without per-statement headings,
and verify tests, builds and localhost. No new worktree.

## Subtle statement boundaries — 2026-10-14

The human accepted subtle boundaries across the shared editor: consistent compact
statement spacing and a muted left-edge highlight on hover or keyboard focus. Keep
full-word kind labels, IDs under Metadata, context-menu deletion and existing text
width. Do not add statement titles or filled cards. The future before/after comparison
view and its comparison-baseline decision remain deferred.

## On-demand statement details — 2026-10-14

The human selected the on-demand details mock and authorized implementation across
the shared editor, updating the existing plan. Replace persistent kind labels and
Metadata rows with an information button on hover or keyboard focus (always visible
on touch). Kind, ID and authored metadata remain inspectable. Keep open questions
and warnings visible. Before/after comparison remains deferred.

The human specified the detail labels **Type** and **ID**, replacing Kind and Statement ID.

The human then moved the icon to the end of the statement and removed the unnecessary
leading indent. This supersedes keeping the former label gutter; preserve the overall
document column and use a small trailing action area.

## Quiet outline — 2026-10-14

The human selected Quiet outline to distinguish statements from surrounding ordinary
text. Use thin, always-visible neutral statement outlines without fills or large
headers. Keep prose/headings unframed, content aligned, the information icon on the
right, Type/ID in on-demand details, and question/warning indicators visible.

## Muted metadata footer — 2026-10-14

The human selected Muted footer and requested implementation across the shared
editor. Expanded details use a divider, soft background and small Metadata heading
inside the statement outline. Keep the right-edge information control, on-demand
Type/ID, authored metadata and links, and visible question/warning indicators.
The human then requested Type and ID beside the Metadata title on the same row.
The human reported that authored source links looked like ordinary text. Make
metadata links visibly recognizable before hover or focus.
The human requested removal of the redundant document-level Metadata section title
because metadata is already shown separately in statement details.
The human flagged the information button's vertical alignment; align it with the
first statement content line for both single-line and wrapped content.
The human also requested recognizable link styling in ordinary document content,
showing the Reading paths list as an example.
The human refined the link treatment to a dotted underline at rest and a solid
line on hover, illustrated by the supplied reference-link screenshots.

## Reference popup presentation — 2026-10-14

The human approved restoring the original popup structure: a compact reference
header, aligned open/close icons, readable contrast and a lightly bordered preview
area. Keep rendered Markdown, with Type, ID and provenance on demand. Loading and
error states use the same structure. This refines current-format presentation,
without restoring legacy graph or ontology compatibility.

The human subsequently requested removing all generated Current draft, Live files
and Committed files labels from the popup; retain Type, ID and authored provenance.

<a id="core-statement-comparison--2026-10-15"></a>

## Core statement comparison — 2026-10-15

The human requested a design for the core before/after comparison in the ontology
editor, using the supplied stacked Proposed/Before screenshot as a visual example.
The proposed core scope uses one Show changes toggle in the full document, stable-ID
statement pairing, editable current content including unsaved edits, read-only prior
content, and added/modified/deleted statement displays. Existing width, statement
format and on-demand metadata remain.

The human chose Diff Graph's All changes baseline exclusively: use each repository's
merge base with the parent workspace, and show unavailable where no parent baseline
exists. Do not use Uncommitted only or follow Diff Graph's selected mode. They
explicitly declined metadata-only change flags and deferred prose comparison.
The surface is Statement changes, not a complete document diff. The initial request
authorized design documents. The human subsequently requested implementation of
the statement comparison design in the existing plan on 2026-10-15.

## Ontology insertion menu — 2026-10-15

The human requested exactly Add Statement and Heading in the ontology-file + menu.
Add Statement always creates an Intent; Heading always creates Heading 2, with no
type or level submenu. Insert after the owning statement or at the selected document
position, not inside a statement body. Project Docs and other files retain their
existing controls. This supersedes the earlier agents/Files-only creation decision.
The human explicitly authorized implementation.

The human rejected the added popup and approved immediate inline insertion instead:
Add Statement focuses the new Intent text after its bullet; Heading inserts H2 with
“New heading” selected, as in Project Docs. Both remain drafts until Save.

The human specified `user.<UUID>` instead of `statement.<UUID>` for IDs generated
by Add Statement.

The human chose to remove the + buttons from ontology statements rather than
move them into the left gutter.

## Statement Enter behavior — 2026-10-15

The human approved Enter at the end of a nonempty ontology statement creating a
new Intent below it with a fresh `user.<UUID>` ID and paired metadata, focused at
its empty text after the bullet. Shift+Enter inserts a newline anywhere; Enter
elsewhere inserts a newline without splitting the statement. Enter on an empty
statement does nothing. Creation is one undoable draft change under ordinary Save.
The human authorized implementation and commit.

The human subsequently requested that an ontology statement with no content be
deleted when its text field loses focus.

## Compare toolbar — 2026-10-15

The human requested moving comparison immediately left of Human and enabling it
by default, then selected the name Compare. The accepted presentation includes
a small divider separating it from filters and a visible selected state. The
human authorized implementation and commit.

## Statement group insertion — 2026-10-15

The human requested implementation and commit of one + at the end of each group
of ontology statements, offering Add Statement and Heading. This refines the
earlier removal of per-statement + controls.

The human clarified that the group + should appear only on hover, like the
other + controls, rather than remain visible at rest.
The human also requested matching the horizontal alignment of other + buttons.

## Stable comparison refresh — 2026-10-16

The human requested retaining the existing comparison during background refresh
to prevent the ontology view blinking, and authorized implementation and commit.

## All ontology changes — 2026-10-16

The human requested a View all changes entry opening changed statements grouped by
ontology file, using current G5 styling. They chose the Continuous page mock, a
read-only overview of saved workspace content including uncommitted changes,
excluding unsaved editor buffers, and authorized updating this plan and implementing.

## Ontology-only presentation — 2026-10-15

The human approved normal bullet-list spacing for ontology statements only, with
one fixed presentation bullet independent of statement text. Show the text
literally, including authored leading `-`, and edit it after the bullet. Apply
this to Current and Before; preserve saved Markdown and other document surfaces.
The human requested no resting border, borders/metadata/change labels after the
bullet, flush metadata without an extra upper gap, and no blue text-edit outline.
These approved corrections supersede the earlier shared persistent-outline rule
for ontology files. Recorded during main integration to reconcile the governing
body with the existing approved implementation and plan.
