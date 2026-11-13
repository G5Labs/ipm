# Diff Graph support for the current ontology — owner decisions (issue #666)

## Human request

[G5Labs/G5-Lean#666](https://github.com/G5Labs/G5-Lean/issues/666), "Fix Diff Graph
to support the current ontology structure, format, and paths". Business Engineering
listed its open decisions and conflicts with recorded intent in the workspace's
Project Docs (`docs/0da22ea3-83bb-4eee-8fe8-7da7886243c3/open-questions.md` in the
docs repository). The owner answered every question on 2026-10-20 as a recorded
decision in the question sets beside it (`.../questions/diff-graph-*.g5q.json`).

## Human decisions — 2026-10-20

Option labels are the owner's selections; quoted text is the owner's free-text answer.

1. Surfaces: the Diff Graph tab, the Review changes dialog's Changed nodes list and
   the rebase/merge preview are all in scope.
2. Ontology files: "Use .g5.md and also change ontology editor to use .g5.md".
   Ontology in any workspace repository counts.
3. Columns: one Ontology column for the whole tree, with folders shown as grouping.
4. Other `.g5` content: "Every Markdown (.md) file under .g5/chat_intent/, at any
   depth, is the new source of Intents → Chats. Any other file under
   .g5/chat_intent/ goes under Intents → Other. These files replace the chat records
   Diff Graph reads today from the old intent_record ledgers. Design plans at
   .g5/pr_ontology_diff_design/<topic>/ontology-plan.md go under Intents → Documents.
   All other .g5/ content outside .g5/ontology/ and .g5/chat_intent/ stays under
   Intents → Other. In the docs repository, question sets
   (docs/<workspace-id>/questions/*.g5q.json) go under Intents → Other, not Intents →
   Documents. Other Project Docs files stay under Intents → Documents."
5. Nodes are files, with statement changes in the details panel; links between
   files are summarised from their statements.
6. A file that cannot be compared reliably is shown with a visible "cannot compare"
   warning; its statements are not classified and the rest of the graph is unaffected.
7. The Human intent lens uses whatever the editor's Human filter uses.
8. Metadata-only changes appear only as a file-level "other changes" note, as today.
   Changes to headings and prose outside statements appear as a file-level "other
   text changed" note. No statement is flagged for either.
9. All changes includes saved but uncommitted ontology edits, matching the editor's
   All changes page.
10. "Uncommitted only" is hidden or disabled until it works, saying so instead of
    showing a false empty state.
11. Relationship display is lifted for Diff Graph only: authored links are drawn and
    labelled as references, not dependencies. Shown kinds: statement-to-statement,
    statement-to-file, formal Expands, footer provenance/evidence links to chat intent
    or design records, links to source code and links to Project Docs.
12. A changed statement's unchanged link targets appear as context nodes.
13. Missing targets: a link to a statement deleted in this comparison points at its
    deleted node; a link to a target that never existed is drawn as a dangling
    "missing target" link. Only links that were themselves edited are reported as broken.
14. Relative links into another repository of the same workspace count; absolute URLs
    remain external.
15. Statements keep identity across files, with one rule for Diff Graph, the editor's
    Compare and All changes. After a move, a link that still names the old path shows
    as broken with a missing-target message. A pure move is hidden unless the text also
    changed and is listed only in the file's details. A statement that moved and
    changed is one statement, both moved and modified, whose Before shows the old
    location and text. An ID appearing in more than one file is flagged ambiguous on
    every copy and none is paired.
16. Navigation: "Clicking a file node opens the details panel, which lists the file's
    changed statements. The panel's Open document opens the file in the ontology
    editor with Compare on." A deleted statement opens at its read-only Before row in
    the editor's Compare view; a deleted file opens in All changes. Target-side changes
    in the rebase/merge preview show only the details panel. Links between nodes stay
    non-clickable. The page address is unchanged.
17. Empty state: "Keep current behavior".
18. Legacy `.semlift` layout and `@ref` support is removed; Diff Graph stops reading
    the `intent_record` and `intent_grounding` ledgers.
19. Automated regression checks with representative current-format files are enough
    evidence to accept the issue.

## Human System Engineering notes — 2026-10-20

The owner answered the two technology notes in the same document:
"One statement parser and pairing rule" for Diff Graph and the editor's Compare and
All changes, and "Git rename detection" for detecting file moves.

## Human decision — uncommitted scope, 2026-10-20

The agent provisionally limited saved uncommitted edits in All changes to ontology
files and explained that, before the Lean cutover (`09f11e34`), the change graph
layered server-side drafts over committed heads for every file. The owner answered:
"Let's apply to every file".

## Human decisions — navigation and scope, 2026-10-20

On the plan's open assumptions, the owner chose: "every changed-statement row in the
details panel opens the editor with Compare at that statement". The owner accepted
("sounds good") that statement identity is scoped to one repository and never pairs
across repositories, and that the rebase/merge preview stays on committed heads and
offers no navigation on either side.

After the agent explained that the server cannot see files changed only by unpublished
local VM commits, the owner confirmed ("Confirm") the fallback: such a repository shows
committed content for non-ontology files with a visible notice, and ontology files stay
fully live. Extending the VM Changes report to list those paths remains the escalation.

The owner chose ("Yes") to make the ontology structural checker
(`.agents/skills/ontology-uplift/scripts/check_ontology.py`) flag a plain `.md` file
under a `.g5/ontology` directory as not an ontology file, while still checking its
structure, instead of leaving it as-is or narrowing it to `*.g5.md`.

## Human decisions — move tracking and comparison owner, 2026-10-20

After the agent raised concerns about Git rename detection (uncommitted renames,
false similarity matches, splits and merges, and two sources of move truth), the owner
proposed keeping the editor's current rule "for now": "pairing is same-file only, and
the ontology said 'no automatic move tracking is required.'" The agent confirmed this
applies to Diff Graph and the editor alike, and the owner agreed ("yes"). This
supersedes decision 15 and the "Git rename detection" note. Issue #666's move criterion
narrows to "navigation still works after a move". A renamed file shows as deleted plus
added, and links to a moved target point at the deleted original.

The owner asked whether a single server-side comparison scales. The agent explained
that it means one server round trip, upload and parse per edit pause in Compare,
and recommended keeping the editor's browser comparison under same-file pairing. The
owner agreed ("yes"). Compare and All changes keep `frontend/src/lib/ontologyComparison.ts`;
Diff Graph gets a matching server implementation; a shared conformance corpus holds
both to one rule. This relaxes the "One statement parser and pairing rule" note.

## Human decision — link scope, 2026-10-21

Reviewing the implementation in Preview, the owner saw many unchanged chat records and
source files drawn as context only because unchanged statements of a changed ontology
file cite them. The agent explained that before the format change Diff Graph showed an
unchanged chat record only through a changed grounding edge, and offered three options.
The owner chose option A ("Let's do A"): links, and the context nodes they add, come
only from changed statements (added, modified or deleted), plus any link that was itself
added or removed. This resolves the plan's assumption A8 and keeps decision 12.

## Agent interpretation

Decision 11 supersedes, for Diff Graph only, the 2026-10-13 editor decisions to defer
dependency visualization and to retain grounding-record tracing
([core ontology decisions](../saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13)).
Same-file pairing, no move tracking and the deferral of automatic redirects remain in
force for both the editor and Diff Graph. The selected design is in the
[plan](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md).
