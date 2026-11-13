# Issue #925 — Core Ontology editor regressions

## Human scope — 2026-10-28

The issue owner's decisions are recorded on
[G5Labs/G5-Lean#925](https://github.com/G5Labs/G5-Lean/issues/925): the
[refined scope](https://github.com/G5Labs/G5-Lean/issues/925#issuecomment-6477652048)
(Preview selection commenting G1, double-click to edit G2, removal of the
per-record Comment action G3, the ontology presentation regressions O1–O4,
Add/Delete statement O5–O6), the
[Source selection-commenting update](https://github.com/G5Labs/G5-Lean/issues/925#issuecomment-6477851067)
(G1a Source, G1b Preview; line anchors with a quoted selection; no inline
highlights) and the
[regressions update](https://github.com/G5Labs/G5-Lean/issues/925#issuecomment-6478516509)
(R1–R11, K1–K4; toolbar and display items deferred to #927).

## Human decisions — 2026-10-29

Context: PR #943 (rich-text editing) merged after the issue was written. It
retired the per-block Preview editor and added a Rich text view beside a
read-only Preview.

- "We want to allow editing in Preview just for ontology files." Asked what
  that meant, the owner first chose Add/Delete statement controls in Preview,
  then withdrew it: "Actually don't add 'Add' and 'Delete' in preview."
- "Let's make the Rich text mode for ontology files specific to ontology. The
  Rich text mode for ontology files shows only the prose without metadata,
  id, ...". The owner accepted the agent's five conditions: hidden lines stay
  in the document, protected and saved verbatim; statement bullets and outline
  stay visible; Backspace/Delete stop at a statement boundary and arrows skip
  hidden lines; diagnostic or unpaired lines stay visible as source; ordinary
  prose and section headings show, and a heading that is a statement's
  identity is hidden.
- "No need for 'Add' and 'Delete'": O5 and O6 are dropped; statements are
  created and deleted in Source.
- The rest of the scope stands: O1–O4, R1–R8, K1–K2 and G1–G3.

## Agent interpretation

- #943 made R9, R10 and K4 moot (no per-block editor remains) and implemented
  most of G2 and K3; the [plan](../../pr_ontology_diff_design/issue-925/ontology-plan.md)
  records the remainder.
- R11 follows from [comment authority](../../ontology/features/documents.g5.md#g5.documents.comments.authority):
  a reader composing a suggestion is reading unpushed text, which only the
  owner may anchor to, so Preview offers that reader no Comment, as Source
  already does.
- Rich text offers no commenting; Preview and Source own it.

## Human decision — reading tools in the file row — 2026-11-02

> Make we make these three Compare, Human, Risk / Quesiton buttons appear in the
> top bar (where the path is shown)?

Human decision: Compare and the Human and Risk/Question filters move from above
the document into the file row, beside its other controls.

Agent interpretation: they show only while the rendered document is read (the
only view they act on), first among the row's controls with a divider before
the mode switch, and take the row's button height so it does not change between
modes.


## Human decisions — 2026-11-02

> In Rich text editor for ontology files, enter at the end of the statement
> should create a new statement. Shift+Enter should create a newline within the
> statement.
>
> I want to allow adding statements in rich editor, as well as deleting.

This supersedes the 2026-10-29 "No need for 'Add' and 'Delete'". The owner
answered the agent's questions:

- Part of this workspace (#925), not a new issue.
- Both the owner and readers; a reader's change is part of their suggestion.
- Statements only, no headings.
- Added with Enter at the end of a statement; Shift+Enter makes a new paragraph
  in the same statement. Inside lists, quotes, code blocks and tables Enter keeps
  its ordinary meaning; Enter on an empty statement does nothing.
- Always an Intent, with a `user.<UUID>` ID and a metadata entry tagged
  `human-provided`. A Question gets no `g5-question` payload.
- Deleted with Backspace in an empty statement, removing its anchor, marker,
  body and metadata entry together. Nothing removes an empty statement
  automatically; an empty statement shows a placeholder ("Type the statement,
  or press Backspace to remove it").
- When other statements expand it or link to it, from any ontology file in the
  repository, deletion warns but is allowed. A dialog appears only when there is
  a warning. (Superseded later the same day; see below.)
- Compare shows the result as Added or Deleted rows; add and delete are refused
  with a pointer to plain text when the structure is ambiguous; each change is
  one undoable step under ordinary Save.
- A reader's added or deleted statement reaches the owner as separate hunk
  suggestions, which the owner could accept partly; grouping them is deferred.

## Agent interpretation — 2026-11-02

- "Ambiguous structure" is a file showing any statement or metadata line for
  repair, or without a Metadata section: the same lines the reading view
  reveals for diagnostics.
- A warning also arises when some ontology files cannot be read for the link
  check, so the person knows the list may be incomplete. (Withdrawn with the
  check below.)

## Human decision — no link check on delete — 2026-11-02

Trying it in the preview, the owner found Backspace did nothing visible: the
check read every ontology file, about 3 seconds there, and each further press
restarted it.

> I don't like that Backspace (an implicit action) triggers the check, which
> can run for a couple of seconds. What if we don't do any checks at all?
>
> Drop the checks.

Human decision: Backspace in an empty statement deletes it at once, with no link
or expansion check and no dialog, as the October editor's deletion did. Links to
it stay as written and show a missing target. The owner also considered dropping
new empty statements on Save and accepted the agent's recommendation not to:
Backspace stays the only way an empty statement is removed, and the placeholder
is unchanged. The deletion is a draft change until Save; Undo and Discard
reverse it.
