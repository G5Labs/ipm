# Documents: Core Ontology editor

The file-backed Core Ontology view: presentation, verification, findings, statement editing and comparison. See [Documents](../documents.g5.md#g5.documents).

## Core Ontology editor

<a id="g5.documents.ontology"></a>
[I · g5.documents.ontology]

Core Ontology is a specialized file-backed view of a repository's ontology files, for reading, editing, comparing and verifying statements.

<a id="g5.documents.ontology.current-format"></a>
[I · g5.documents.ontology.current-format]

Ontology files are the `*.g5.md` files at any depth under `.g5/ontology` in any workspace repository; other files there are ordinary files. Core Ontology and [Diff Graph](../diff_graph.g5.md#g5.diff-graph.columns) read them and support the [current authoring format](../../../../.agents/guidance/ontology-format.md) — explicitly anchored stable I/D/Q IDs, natural section headings and per-block footer metadata.

- An anchor line directly above a statement's own anchor is an alias for a former ID: links to it reach the statement, and it is an identity line, not part of the previous statement. An alias that repeats any statement's ID refuses the statement as ambiguous.
- The file identity, captured-reference and selected-file Save contracts above govern this extension.

<a id="g5.documents.ontology.navigation"></a>
[D · g5.documents.ontology.navigation]

Core Ontology uses shared authorized file listings, reads and selected-file Save with workspace/repository/path identity, and the shared [durable authored-content policy](files.g5.md#g5.documents.durable-authored-content) for Files repair, creation and Save. Its file opens use the [common editor](editor.g5.md#g5.documents.editor.experience).

- Ontology controls in Preview, Compare and All Changes are specialized presentations over these files, available independently of the opener, with no node UUID, projection storage or synchronization worker.
- The tree shows repository → folders → files, only for repositories with ontology under `.g5/ontology`; one qualifying repository shows its ontology directly, several group by repository.
- Apply [captured-source truth](files.g5.md#g5.documents.captured-references) to discovery as well as content: the attached owner's new live files appear through existing refreshes, and incomplete or failed discovery is visible and cannot classify a repository as empty or establish that exactly one qualifies.

<a id="g5.documents.ontology.presentation"></a>
[D · g5.documents.ontology.presentation]

Core Ontology uses the [common rich and plain text modes](editor.g5.md#g5.documents.editor.modes). The rendered document shows no generated statement titles and offers no title, ID or structural editing: prose is edited and extended in [Rich text](editor.g5.md#g5.documents.preview.rich-editing), which hides identity, anchor and metadata lines.

- Rich text adds and deletes whole statements under [statement creation](#g5.documents.ontology.creation); every other change to those lines or the structure happens in plain text.
- Across the shared editor and read-only statement frames, a statement's Type and ID appear in on-demand details with its authored metadata, reached by an accessible info button and never mutating source; permanent kind labels and Metadata summaries stay hidden, and human provenance appears in details and the [Human filter](#g5.documents.ontology.human-statements), not as a badge.
- Expanded details are a muted metadata footer that preserves authored metadata and links without reinterpreting them, keeps unmatched or diagnostic metadata visible, and omits the document-level Metadata heading once its entries appear in statement details.
- Source edits may change headings, IDs and metadata, which Preview never normalizes or repairs; provenance stays as authored, without implying verification or granting permissions.
- Authored links in prose, lists, statements and metadata stay recognizable and keep their destinations and navigation.
- Statements are compact and document-like, framed distinctly from ordinary prose, which stays unframed with its section headings; statement text stays within the [text measure](editor.g5.md#g5.documents.text-measure).
- For ontology files only, statements form one bullet list using the [disclosure markers](#g5.documents.ontology.disclosure), with statements, properties and child statements at the same text size; a changed statement's Current and Before rows are one item meeting without a gap, a deleted statement is its own item, and authored text, including a leading `-`, shows literally without an inserted marker.
- A trailing action area holds the [verification status](#g5.documents.ontology.verification.status) and on-demand actions, so revealing them does not shift text.
- Question status, severity, blocking, uncertainty and format warnings stay visible beside the lead, appearing when a closed fold opens, and existing review access is kept. A statement's kind is not a tag: a Q statement shows no Question badge, its Type says it in details, and no Open or Answered state is invented from its kind.
- Comment and question actions keep their current authority, and authored tags are edited in plain text. Generated outlines use kind rather than titles or IDs, and linked-block previews identify the document path and disclose Type and ID under Metadata.

<a id="g5.documents.ontology.disclosure"></a>
[D · g5.documents.ontology.disclosure]

In Preview, each top-level ontology statement shows its first paragraph, and one triangle opens the rest of the statement together with its child statements.

- A statement with nothing more to show has a dot instead, only slightly smaller than the triangle. Markers are black, or neutral in dark mode, and never show severity or evidence.
- Folds start closed and show nothing while closed. Expand all and Collapse all act on every fold; closing one after Expand all leaves the others open.
- A child's ID extends its parent's ID after a dot, and it follows the parent or the parent's earlier children with no section heading between. Other statements are not nested, whatever their IDs. Children read as sub-bullets with their own controls and properties, without folds of their own.
- Navigating to content in a closed fold opens it. Review suggestions, Compare changes and filter matches inside a fold open it too, even one the reader had closed before they appeared; margin cards for text in a closed fold sit level with its statement.
- Rich and plain text show the whole document at their usual text size, so entering editing reflows ontology statements.

## Verification and findings

<a id="g5.documents.ontology.verification"></a>
[I · g5.documents.ontology.verification]

Each ontology statement shows its verification status, and opening the status shows the saved evidence behind it.

<a id="g5.documents.ontology.verification.status"></a>
[D · g5.documents.ontology.verification.status]

A statement's status comes from its one verification tag, written as a bullet of the tag alone: Test Coverage, Manual checking and review, Design verified or Implementation verification. Without a tag, or with two different tags, it is Unverified, shown quietly.

- Design verified also needs a linked state machine with a manifest and a recorded run; until that is known the status reads Checking…, and without it, or when the evidence cannot be read, the statement shows Unverified. Freshness and applicability are not checked.
- Unverified and Checking… open no details.
- A statement with a Design verified descendant shows Design verified, unless its own status is Implementation verification, and its details name those descendants. Status never passes down: a child shows its own.
- Design verified shows a small dot after its text when the statement or a descendant links a Critical, High or Medium [finding](#g5.documents.findings) not marked Irrelevant, coloured by the most severe of them and named in its accessible label. Low and Unranked findings show none.
- Other tags, including `future`, do not set a status.

<a id="g5.documents.ontology.verification.details"></a>
[D · g5.documents.ontology.verification.details]

For each linked state machine of a design-verified statement, the details show its title and saved checking question, then on request its saved bindings and recorded runs. For Test Coverage, Manual checking and review or Implementation verification, the details show what the statement's metadata records, without its tag bullets.

- A link to any file in a state machine's folder under `TLA_verification/models/` links that state machine; links in code examples do not. Only the linked state machines' saved files and the run index are read, directly, so the folder need not be visible in Files. Their content is shown as saved, without generated summaries, and a failed read offers Retry.
- Each declared run shows its latest execution with its kind, expectation and outcome; earlier and retired runs remain available. A result that matched its expectation is green; incomplete and unexpected results look different.
- The details of a design-verified statement also list the [findings](#g5.documents.findings) its metadata links, each with its title, severity and decision, opening the entry. Status changes are not offered.

<a id="g5.documents.findings"></a>
[I · g5.documents.findings]

After a formal verification run, the workspace owner reviews the bugs/issues and design simplifications it found but did not fix, and decides for each actionable one whether it becomes a tracker issue and with what priority.

<a id="g5.documents.findings.files"></a>
[D · g5.documents.findings.files]

Findings live in `TLA_verification/bugs_issues.md` and `TLA_verification/design_simplification.md`, the folder whose verification material the viewer reads. They belong to the repository, not to a workspace, and are committed like ordinary content. Every later run, such as an uplift or a [Base re-verification](../ontology_and_harness.g5.md#g5.reverification), extends these same two files in place and leaves decided entries intact.

- Each entry has a stable anchor, a one-sentence title and a severity: Critical, High, Medium or Low, otherwise Unranked.
- Each entry names the run that first recorded it on a `Run:` line, such as an uplift or a Base re-verification with its date. Entries recorded before the line existed have none.
- An actionable entry, one the owner may want tracked, also has a decision line: a defect, a finding that needs a product or design decision first, or a simplification still proposed. Other entries are listed without one. Severity does not decide which.
- The Core Ontology panel lists the files that exist, after the ontology files and with their own symbol; nothing else lists them, and links from the ontology open their entries.
- The reader filters entries by severity, with toggles that read as a filter, and by run: all runs by default, one run, or the entries that name none. While it filters, it shows only the matching entries and the headings that introduce them, without the description around them. Entries keep the file's order, with actionable entries first, most severe first.
- Each entry reads as a card edged in its severity's colour. Its decision shows Open in green, Need fix in red, Defer in amber and Irrelevant in grey, and when the buttons are unavailable the page says why once.

<a id="g5.documents.findings.evidence"></a>
[D · g5.documents.findings.evidence]

Each finding cites its evidence, not the older reports it replaces. The reader shows runs as the run index records them, and never pairs runs itself: the material says which run checks which model.

- A bug names the state machine runs that found it and the test or probe that reproduced it. Without a reproduction, the reader shows the reproduction heading alone, in bold.
- A simplification names a run of the current design's model and a run of the simplified model. The reader shows both runs' recorded outcomes and explored states at the end of the entry.
- When both runs record valid state counts and the current design's count is greater than zero, the reader shows the ratio and percentage change.
- If the entry notes differences in scope, checked properties or behavior, or either run failed or did not finish, the reader explains the limit alongside the score. The score measures recorded states; it does not establish correctness or application performance.

<a id="g5.documents.findings.decision"></a>
[D · g5.documents.findings.decision]

The owner of a running workspace decides an actionable entry in one press — Need fix, Defer or Irrelevant — and an untrusted GitHub destination first shows exactly what will be sent, as shared issue creation does. The entry's decision line is the only record, written into the workspace's checkout and committed with its other changes, not on its own.

- Need fix and Defer create one issue in the organization's creation tracker through shared [issue creation](../issues.g5.md#g5.external-issues.creation). Its title is `[High]` or `[Low]` and the entry's title, and it holds the entry as displayed, with links into the deciding workspace. The issue gets no parent, assignee or workspace link, and the decision line links it.
- Irrelevant records that the user decided the entry is not important, and creates nothing.
- Changing a decision keeps the same issue: it changes the title's priority, closes the issue for Irrelevant, and reopens it for Need fix or Defer.
- The tracker changes first and the line is written after. A press whose write fails says so and offers Retry. A later press by the same person for the same entry finds its issue through a per-entry creation key rather than creating another, within the tracker's lookup limits.
- Every entry offers a chat action that seeds a fresh chat with the entry beside the document, as Discuss does under [chat actions](../ui.g5.md#g5.workbench.chat-actions).

## Editing and navigation

<a id="g5.documents.ontology.creation"></a>
[D · g5.documents.ontology.creation]

Create, edit, move and delete ontology statements or headings in plain text under ordinary Save. The rendered document supplies reading and navigation, while [Rich text](editor.g5.md#g5.documents.preview.rich-editing) edits prose within statements and adds or deletes whole statements, for anyone who may edit the file or suggest changes.

- Enter at the end of a nonempty statement whose last block is a paragraph adds an Intent below it with a fresh `user.<UUID>` ID, its anchor and a paired human-provided metadata entry; elsewhere Enter keeps its ordinary meaning, and on an empty statement it does nothing.
- Backspace in a statement with no content deletes its anchor, identity lines, body and metadata together, at once and without confirmation; links to it stay as written and show a missing target. Nothing else deletes an empty statement.
- Each addition or deletion is one undoable draft change under ordinary Save, or reaches the owner through Push review as ordinary suggestions that may be accepted separately.
- Both are refused, pointing to plain text, while the file shows a statement or metadata line for repair or has no Metadata section.
- Rich text never moves statements, changes their type or ID, adds headings or offers insertion menus; any other edit that would add, remove or duplicate part of a statement's anchor, identity or metadata lines is refused.
- Agent file edits follow the [authoring rules](../../../../.agents/guidance/ontology-format.md#blocks-and-ids) for stable IDs, paired metadata and links.

<a id="g5.documents.ontology.migration"></a>
[D · g5.documents.ontology.migration]

Support the current ontology format without legacy ontology discovery, fallback or
compatibility handling. Legacy ontologies are migrated manually.
[Legacy graph compatibility](#g5.documents.ontology.graphs) is also excluded; no
legacy ontology loader or automatic conversion is required.

<a id="g5.documents.ontology.links"></a>
[D · g5.documents.ontology.links]

Open relative document and source links inside G5 in their owning repository, jumping to the heading, anchor or line named. A statement or heading target opens the rendered reading view from either mode, leaving editing with the draft kept as Done does, and jumps there once per request with a brief highlight; later content or comparison changes do not move the reading position.

- Footer provenance, evidence and implementation links are clickable and keep their authored meaning; absolute URLs keep their destinations.
- A relative target that does not exist is tried from the repository root and then the `.g5` folder, opening the first file that exists; a destination with unencoded spaces is a link too, and a link that still finds no file says so and names the paths tried.
- A followed link offers trace navigation's existing Back, which returns once to the statement or finding holding it, or to the top of the document for a link outside them.
- Existing [tenant authority](../../concepts/security.g5.md#g5.security.tenant.rls) governs navigation; a link confers no additional access.

<a id="g5.documents.ontology.previews"></a>
[D · g5.documents.ontology.previews]

A reference popover previews a linked ontology block, with Open document navigation to its target; a whole-document link opens the document directly. The preview shows rendered Markdown under a document-path header, with Type, ID and authored provenance available on demand after the content.

- Omit generated content-source labels (Current draft, Live files and Committed files).
- Keep the same header and container for loading and errors, with close available and other actions disabled.
- Preserve repository-qualified navigation and the existing outside-click and Escape dismissal.

<a id="g5.documents.ontology.graphs"></a>
[D · g5.documents.ontology.graphs]

Support current-format [reference navigation](#g5.documents.ontology.links) and [block previews](#g5.documents.ontology.previews), but exclude legacy graph compatibility; the editor draws no relationship graph.

- Ordinary citation or evidence links do not necessarily declare dependencies, so Diff Graph draws authored links as [typed relationships](../diff_graph.g5.md#g5.diff-graph.relationships), never as dependencies.
- The retired combined resolved-context panel remains outside this change.

<a id="g5.documents.ontology.tracing"></a>
[D · g5.documents.ontology.tracing]

Remove record-trace Source and Drives controls and trace panels from the shared editor and read-only statement frames on every document surface. Current ontology uses authored Markdown and footer metadata, not separate intent or grounding ledgers.

- Preserve ordinary links and block previews without inferring dependency or trace edges.
- Independent All intent logs tracing and backend ledger readers stay outside this UI change; [intent history](discussion.g5.md#g5.documents.intent-history) applies there.

<a id="g5.documents.ontology.moves"></a>
[D · g5.documents.ontology.moves]

When agents move ontology, the [authoring rules](../../../../.agents/guidance/ontology-format.md#blocks-and-ids) require preserving block IDs, moving their metadata and updating affected Markdown links. The sidebar refreshes to the new file location and the editor follows updated links.

- Stale links or tabs show a clear missing-target message.
- Automatic redirects, relocation of open tabs and transfer of comments are deferred; no automatic move tracking is required here.

<a id="g5.documents.ontology.human-statements"></a>
[D · g5.documents.ontology.human-statements]

A statement is human when one of its footer metadata bullets consists only of provenance flags and includes `human-provided` or `human-verified`, or when a Question's embedded payload carries an answer a person recorded. Agent provenance and agent-proposed answers are not human.

- The editor's Human filter and Diff Graph's [Human intent lens](../diff_graph.g5.md#g5.diff-graph.human-lens) both use this rule.

## Comparison

<a id="g5.documents.ontology.statement-changes"></a>
[D · g5.documents.ontology.statement-changes]

Offer an optional Statement changes view in the current-format ontology editor, using one Compare toggle on by default when a document opens, placed left of Human and divided from the filters. Keep the full current document and its text width, showing a modified statement's current version above its read-only Before version, with additions and deletions labelled and deleted statements shown only as a read-only prior version.

- Preserve [shared presentation and editing](#g5.documents.ontology.presentation), on-demand Type/ID/provenance and visible questions and warnings.
- Current content includes the open editor's unsaved edits; toggling or refreshing comparison never saves, discards or replaces them.
- Keep the last successful comparison visible during background refresh of the same baseline, marked as updating until replacement.
- With Compare on, a navigation target naming a statement deleted in the comparison opens at its read-only Before row; with Compare off it is a missing target.
- [Change highlights](#g5.documents.ontology.change-highlights) mark what changed within a modified statement and shorten its Before version.
- Defer prose and heading comparison, changes-only filtering inside the editor, and new review or approval controls.

<a id="g5.documents.ontology.change-highlights"></a>
[D · g5.documents.ontology.change-highlights]

As part of [Statement changes](#g5.documents.ontology.statement-changes), mark what changed inside a modified statement: align the two versions' sentences, then the changed words. The Current row highlights added sentences and words and the Before row the removed ones, by colour only, with the row labels saying which is which and unchanged text plain.

- Compare the text a reader sees, so Markdown syntax and line wrapping are not changes.
- Show the pair without highlights when too little text survives, when the visible text is identical, when a pending suggestion's preview stands in for part of the current text, or when either version exceeds 2,000 words or is otherwise too large to compare promptly.
- To save space, a highlighted pair shows less of its Before version until the reader asks for all: no Before row when nothing was removed, otherwise only the Before sentences that lost text, with an ellipsis for the rest.
- The Show-all control sits at the bottom of a row's label column, on the Current row when Before is hidden and the Before row when it is shortened; showing or hiding Before leaves it in place, and shortening leaves the statement where it is. A pair without highlights, or whose every Before sentence lost text, shows Before whole.
- Highlights are presentation only: they never change content, selection, copying, comments, links or editing, nor the [matching rule](#g5.documents.ontology.comparison-matching)'s labels, and without browser support the pair shows without them.

<a id="g5.documents.ontology.all-changes"></a>
[I · g5.documents.ontology.all-changes]

View all changes opens a continuous read-only page of changed statements grouped by ontology file across workspace repositories, including deleted files. It uses the [comparison baseline](#g5.documents.ontology.comparison-baseline) and [matching rules](#g5.documents.ontology.comparison-matching), with current content from saved workspace files including uncommitted edits but excluding unsaved browser buffers.

- Reuse comparison presentation and metadata, counting a modified pair once.
- Open file returns to the editor for that exact repository and path; a request naming a file, including a deleted one, opens the page at that file.
- Keep successful rows during background refresh, and make unavailable or incomplete results explicit instead of reporting a complete zero or total.

<a id="g5.documents.ontology.comparison-baseline"></a>
[D · g5.documents.ontology.comparison-baseline]

Statement comparison uses the same per-repository merge-base selection and access rules as Diff Graph All changes, independent of the graph's mode, and shows the comparison scope and selected baseline revision. It captures immutable source and parent revisions and reads the baseline file at the selected commit.

- Require a readable parent and compatible repository membership; no parent, missing history or inaccessible inputs make comparison unavailable, without falling back to accepted head or another repository.
- Refresh changed inputs coherently, without mixing old and new versions or disturbing editor drafts.
- Ordinary viewing and editing stay available when comparison fails; no baseline-management or approval state is added.

<a id="g5.documents.ontology.comparison-matching"></a>
[D · g5.documents.ontology.comparison-matching]

Pair statements by stable ID within the same repository and file, and compare authored statement bodies — excluding identity markers, alias anchors, section headings and paired footer metadata — so body additions, modifications and removals determine the change labels. Metadata-only changes, including Type-only changes, do not flag a statement as changed.

- Show each displayed version's own Type, ID and authored metadata on demand.
- Do not infer identities from similar wording, match across files, or treat moves as body changes.
- Refuse ambiguous or malformed comparisons visibly rather than guessing; a file's confirmed absence at an available baseline means added content, while an unavailable baseline is not an empty file.
- Retain repository-qualified current links; Before content must never present current content as a historical target.

<a id="g5.documents.ontology.comparison-owner"></a>
[D · g5.documents.ontology.comparison-owner]

One statement rule, the [matching rule](#g5.documents.ontology.comparison-matching), governs Compare, All changes, Diff Graph and the rebase/merge preview through two implementations. The editor compares in the browser, so Compare reflects unsaved edits with no server work per edit, while Diff Graph and the rebase/merge preview compare on the server, where their links, lens and conflicts are computed.

- A shared current-format corpus fixes the expected statement boundaries, diagnostics and per-statement Added, Modified and Deleted results, and both implementations must pass it unchanged.
- Adopting cross-file pairing later needs repository-wide inputs and revisits this split.

## Metadata

### g5.documents.ontology

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.ontology` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Current format](#g5.documents.ontology.current-format), [Navigation](#g5.documents.ontology.navigation), [Presentation](#g5.documents.ontology.presentation), [Disclosure](#g5.documents.ontology.disclosure), [Verification](#g5.documents.ontology.verification), [Creation](#g5.documents.ontology.creation), [Migration](#g5.documents.ontology.migration), [Links](#g5.documents.ontology.links), [Previews](#g5.documents.ontology.previews), [Graphs](#g5.documents.ontology.graphs), [Tracing](#g5.documents.ontology.tracing), [Moves](#g5.documents.ontology.moves), [Human statements](#g5.documents.ontology.human-statements), [Statement changes](#g5.documents.ontology.statement-changes), [Change highlights](#g5.documents.ontology.change-highlights), [All changes](#g5.documents.ontology.all-changes), [Comparison baseline](#g5.documents.ontology.comparison-baseline), [Comparison matching](#g5.documents.ontology.comparison-matching), [Comparison owner](#g5.documents.ontology.comparison-owner).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.ontology.current-format

- Integration: [Ben ontology decision](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#ben-ontology-integration--2026-10-14); scoped parser and system verification belongs to the existing plan.

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Human planning request, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md); format details refer to the existing authoring contract.
- Implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).
- [Issue #666 owner decision 2, 2026-10-20](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20) narrows ontology files to `*.g5.md` for the editor and Diff Graph. Implemented by `is_ontology_path` (`g5_contracts::g5_paths`) and `isCurrentOntologyPath`, used by the sidebar, editor, Files-backed discovery, All changes and Diff Graph; the structural checker flags plain `.md` pages. See [evidence](../../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).
- Alias anchors for former IDs: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md), decision 13; [verification display design](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md).

### g5.documents.ontology.navigation

- [Unified editor amendment](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md), 2026-10-23: the same ontology file exposes its specialized controls from every entry point. Routing still depends on `TabItem.presentation`; implementation is outstanding.
- [File-backed integration decision, 2026-10-19](../../../chat_intent/saehan-499-ontology-editor/file-backed-integration.md). Earlier node-based implementation evidence below is historical; fresh file-based API, editor and browser verification is recorded in the [integration amendment](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#file-backed-integration--2026-10-19).

- human-provided, agent
- Expands: [Ontology](#g5.documents.ontology).
- [Human decisions, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13).
- Agent refinement: [implementation readiness verification](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-readiness-verification--2026-10-13) closes source-selection, incomplete-discovery and paired-edit guarantees within the selected scope.
- Implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).

- Agent refinement: [shared path classification discovery](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-discovery--shared-path-classification).

### g5.documents.ontology.presentation

- [Source-only amendment](../../../chat_intent/workspace-colab-harden/unified-editor.md#source-only-editing--2026-10-24): source editing replaces rich statement editing/deletion and the earlier prohibition on source mode in Core Ontology. Preview retains statement presentation, details and domain actions. Older rich-edit implementation evidence below does not establish this migration.
- [Rich editing](editor.g5.md#g5.documents.preview.rich-editing) (2026-10-29) lets statement prose be edited and extended in place as a rendered document; identity, anchor and metadata lines stay opaque and plain-text-only. It supersedes the 2026-10-27 per-block source editor.
- The owner's [decisions of 2026-11-02](../../../chat_intent/issue-925/decisions.md#human-decisions--2026-11-02) add whole-statement add and delete in Rich text under [statement creation](#g5.documents.ontology.creation).
- The owner's [one-surface decision of 2026-10-30](../../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30) removes the Preview tab while editing; Done returns to the statement frames and Compare with the draft applied.
- human-provided, agent
- Expands: [Ontology](#g5.documents.ontology).
- [Human decisions, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13).
- [Human statement-editing correction, 2026-10-14](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-correction--2026-10-14) supersedes statement-title display and human move/ID-renaming assumptions. Authored headings remain navigation targets; source repairs use existing file tools.
- Agent refinement: [implementation readiness verification](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-readiness-verification--2026-10-13) closes source-selection, incomplete-discovery and paired-edit guarantees within the selected scope.
- Agent refinement: [revision verification](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#revision-verification--2026-10-14) makes hidden-anchor presentation and safe, undoable deletion explicit; the creation decision below is now settled.
- Prior implementation evidence (before shared-editor simplification): implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).

- [Shared editor simplification](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#shared-editor-simplification--2026-10-14) supersedes visible IDs and editor trace controls; verification is recorded in the existing plan.

- [Compact shared statements](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#compact-shared-statements--2026-10-14) governs layout and context-menu deletion. Verification is recorded in the existing plan.

- [Subtle statement boundaries](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#subtle-statement-boundaries--2026-10-14) governs shared hover/focus boundaries; before/after comparison is deferred.
- [On-demand statement details](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#on-demand-statement-details--2026-10-14) supersedes persistent kind labels and Metadata summaries; question and warning signals remain visible.
- [Quiet outline](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#quiet-outline--2026-10-14) supersedes the hover-only left edge with a persistent statement boundary.
- [Muted metadata footer](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#muted-metadata-footer--2026-10-14) distinguishes expanded details from statement content.
- [Ontology-only presentation corrections](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#ontology-only-presentation--2026-10-15) supersede the persistent outline and Markdown-rendered body for ontology statements.
- [Issue #925](../../../chat_intent/issue-925/decisions.md) (2026-10-28/29): no permanent Human badge (O3); Rich text shows ontology prose only.
- Issue #925 presentation fixes (O3, R5, R6; comparison rows under [statement changes](#g5.documents.ontology.statement-changes)): `SemanticRecordFrame` drops the Human badge; the popover takes `g5-document-links`; `index.css` resets the statement block grid's trailing margin. Tests: `OntologyComparison.test.tsx`, `CurrentOntologyReference.test.tsx`, and the measured layout journey in `e2e/mock-preview/file-editor.spec.ts` (fixture `?ontology&changed`, `?ontology&all-changes`).
- Disclosure markers and the verification status action: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md); [verification display design](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md).
- The owner's [preview review of 2026-11-02](../../../chat_intent/ontology-formal-verification/findings-triage.md#preview-review-2-november-2026), decision 24, drops the Question badge a Q statement showed for its kind: "Question is not a tag, uncertain is."

### g5.documents.ontology.disclosure

- human-provided, agent
- Expands: [Ontology](#g5.documents.ontology).
- Source: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md), decisions 2, 8 and 15; layout from the owner's UI handoff in Project Docs. Design: [verification display design](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md).

### g5.documents.ontology.verification

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- Source: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md), decision 2.
- Children: [status](#g5.documents.ontology.verification.status); [details](#g5.documents.ontology.verification.details)

### g5.documents.ontology.verification.status

- human-provided, agent
- Source: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md), decisions 4, 5, 7, 12 and 16. Design: [verification display design](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md).
- Status rolls up through every level of descendants; that a child never takes its parent's status is decision 7.
- The severity dot is the owner's [decision 29](../../../chat_intent/ontology-formal-verification/findings-triage.md#gaps-against-the-mock-3-november-2026), which replaces the handoff's rule against issue dots on verification buttons.
- Expands: [verification](#g5.documents.ontology.verification)

### g5.documents.ontology.verification.details

- human-provided, agent
- Source: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md), decisions 1, 5, 9 and 17; panel layout from the owner's UI handoff. Linked findings: [findings decisions, 2026-11-01](../../../chat_intent/ontology-formal-verification/findings-triage.md), decision 13. Showing Implementation verification like decision 17, and which metadata lines count as details, are agent choices recorded there. Design: [verification display design](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md).
- Expands: [verification](#g5.documents.ontology.verification)

### g5.documents.findings

- human-provided
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: [findings decisions, 2026-11-01](../../../chat_intent/ontology-formal-verification/findings-triage.md), user story and decisions 1 and 13. Design: [findings triage](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md#findings-triage).
- Children: [files](#g5.documents.findings.files); [evidence](#g5.documents.findings.evidence); [decision](#g5.documents.findings.decision)

### g5.documents.findings.files

- human-provided, agent
- Source: [findings decisions, 2026-11-01](../../../chat_intent/ontology-formal-verification/findings-triage.md), decisions 1, 5, 6, 8, 10, 15, 19, 21, 23, 25, 26, 30–32 and 34–36. Filtering rather than reordering and keeping the headings that introduce shown entries are agent choices recorded there.
- Implementation: [format checker](../../../../.agents/guidance/scripts/check_findings.py), `frontend/src/lib/findings.ts`.
- Expands: [findings](#g5.documents.findings)
- Extending the same files for every run, and the filter by run, are the owner's decision 14 in the [re-verification chat intent](../../../chat_intent/periodical-reverification/request.md) (2026-11-06). The `Run:` line is the agent's means. It sits on its own line because a decision write anchors on the end of the status line. Implemented in `lib/findings.ts` (`findingRuns`), `components/files/FindingsControls.tsx` and `findingsMarkup.ts` (headings list their entries so both filters combine); the shared fixture `tests/fixtures/findings/runs/` holds the checker and the viewer to the same reading.

### g5.documents.findings.evidence

- human-provided
- Expands: [findings](#g5.documents.findings)
- Split from [files](#g5.documents.findings.files) on 2026-11-06 to meet the writing rules, with the same source: [findings decisions, 2026-11-01](../../../chat_intent/ontology-formal-verification/findings-triage.md). The owner's [state-count follow-up](../../../chat_intent/pairsoft-verification-followup/request.md) governs when to show and qualify the score.
- Implementation: [format checker](../../../../.agents/guidance/scripts/check_findings.py), `frontend/src/lib/findings.ts`, and `compareStates` in `frontend/src/lib/verificationArtifacts.ts`.

### g5.documents.findings.decision

- human-provided, agent
- Source: [findings decisions, 2026-11-01](../../../chat_intent/ontology-formal-verification/findings-triage.md), decisions 2, 3, 4, 7, 12, 18, 22 and 27. Reusing one issue across changed decisions, and a chat action on every entry, are agent choices recorded there. The pattern follows [meeting decisions](workbench.g5.md#g5.workbench.meeting), whose question sets cannot hold repository-owned decisions.
- Expands: [findings](#g5.documents.findings)

### g5.documents.ontology.creation

- The [source-only decision](../../../chat_intent/workspace-colab-harden/unified-editor.md#source-only-editing--2026-10-24) supersedes rich insertion, continuation and empty-statement cleanup below. Source-based creation is the target; migration is not implemented.
- The owner's [rich-editing decision of 2026-10-29](../../../chat_intent/rich-text-editing/request.md#human-decision--rich-editing-in-preview--2026-10-29) lets Rich text edit and add prose within statements; statement creation, deletion and structure remain Source operations.
- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Ontology insertion menu, 2026-10-15](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#ontology-insertion-menu--2026-10-15) supersedes the agents/Files-only creation decision.
- [Statement Enter behavior](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#statement-enter-behavior--2026-10-15) defines keyboard continuation and in-statement newlines.
- Implementation and verification: [insertion amendment](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#ontology-insertion-menu--2026-10-15).
- [Issue #925](../../../chat_intent/issue-925/decisions.md#human-decisions--2026-10-29), 2026-10-29: the owner considered and declined Add and Delete statement in the rendered document or Rich text; creation and deletion stay in plain text.
- The owner's [decisions of 2026-11-02](../../../chat_intent/issue-925/decisions.md#human-decisions--2026-11-02) supersede that: Rich text adds statements on Enter and deletes empty ones on Backspace, without checking or warning about links (a link check on Backspace was tried and dropped the same day); design in the [plan](../../../pr_ontology_diff_design/issue-925/ontology-plan.md#rich-text-statement-add-and-delete--2026-11-02). Grouping a reader's added or deleted statement into one suggestion is deferred.
- Implemented by `statementUnits`, `keepsStructure` (the guard), `addStatement`, `deleteStatement` and the `OntologyStatements` keys and placeholder in `richExtensions.ts`; the empty body for a statement without prose in `markdownDocument.ts`; the line ending and refusal notice in `RichMarkdownEditor.tsx`. Tests: `RichMarkdownEditor.test.tsx` and the `file-editor` browser spec (Enter adds, Save writes the exact file, Backspace deletes at once, Compare shows Added; a reader's added statement goes out in Push review with its metadata). Not checked against the full stack with a second, non-owner account.

### g5.documents.ontology.migration

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Human decisions, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13).
- Implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).

### g5.documents.ontology.links

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Human decisions, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13).
- Implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).
- [Issue #925, R1 and K1](../../../chat_intent/issue-925/decisions.md#human-scope--2026-10-28): targets open the rendered reading view from any mode and jump once with a brief highlight; [plan](../../../pr_ontology_diff_design/issue-925/ontology-plan.md).
- Implemented in `FileView` (leaves editing and selects the rendered mode per unhandled request) and `DocumentPreview` (resolution by `ontologyFragmentIndex`, once per request, 2 s highlight); tests in `DocumentPreview.test.tsx`, `FileView.test.tsx` and `file-editor-925.spec.ts`.
- Line fragments, fallbacks from the repository root and `.g5`, and links with unencoded spaces: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md), decision 10; [verification display design](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md).
- Back after a followed link: [findings decisions](../../../chat_intent/ontology-formal-verification/findings-triage.md#preview-review-2-november-2026), decision 28, reusing the Back of trace navigation (`navigationOrigin` in `pages/WorkspacePage.tsx`, the button in `components/editor/CodeEditor.tsx`).

### g5.documents.ontology.previews

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Human decisions, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13).
- [Reference popup presentation](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#reference-popup-presentation--2026-10-14) restores the original popup hierarchy while retaining rendered Markdown.
- Implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).

### g5.documents.ontology.graphs

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Human correction, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-correction--2026-10-13) supersedes legacy graph preservation: compatibility is excluded and dependency visualization deferred.
- Implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).
- [Issue #666 owner decision 11, 2026-10-20](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20) lifts the deferral for Diff Graph only: links are drawn as typed references, not dependencies. The editor is unchanged.

### g5.documents.ontology.tracing

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- The [unified editor design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md) distinguishes the retired record-trace Source control from the retained raw-source editor mode; trace controls remain retired.
- [Human decisions, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13).
- Prior implementation evidence (before shared-editor simplification): implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).

- [Shared editor simplification](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#shared-editor-simplification--2026-10-14) supersedes visible IDs and editor trace controls; verification is recorded in the existing plan.

### g5.documents.ontology.moves

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Human decisions, 2026-10-13](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13).
- Implemented in the working tree · API/component and real Chromium process-mode checks passed; microVM validation remains unavailable. See [scoped evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#implementation-evidence--2026-10-14).
- Issue #666 keeps this deferral for the editor and Diff Graph: [move tracking deferred](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--move-tracking-and-comparison-owner-2026-10-20).

### g5.documents.ontology.human-statements

- human-provided, agent
- Expands: [Ontology](#g5.documents.ontology).
- [Issue #666 owner decision 7](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20) makes the editor's Human filter the shared meaning. The rule records the editor's existing behavior (`currentOntology.ts` provenance tags and `answeredQuestionRecordStarts` in `questionAnswerWrite.ts`, consumed by `DocumentPreview.tsx`).
- Implemented on the server in `g5_semantic::ontology_comparison`; the editor's half uses `questionAnswerWrite.ts::answeredQuestionRecordStarts`. The shared corpus holds both to the same results, including continuation lines and fenced bullets. See [evidence](../../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.documents.ontology.statement-changes

- agent
- Expands: [Ontology](#g5.documents.ontology).
- Toolbar name, placement and default: [human request](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#compare-toolbar--2026-10-15).
- Derived from the [core comparison request and decisions](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#core-statement-comparison--2026-10-15).
- Implemented in the working tree; real localhost browser checks cover unsaved pairing, Save/reload, current navigation/comments, deletion/Undo and competing-write conflict recovery. See [scoped implementation evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#comparison-implementation-and-verification--2026-10-15).
- [Issue #666 owner decision 16](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20) adds the deleted-statement navigation target. Implemented in `DocumentPreview` / `OntologyBeforeStatement` (`data-ontology-deleted-id`); a browser journey opens a deleted statement at its Before row. See [evidence](../../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).
- Issue #925 (O1, O2, O4, R7, R8): the reading view's toolbar styles Compare/Human/Risk/Question via `files/previewToolbar.ts` with a divider and neutral or unavailable Compare titles; `OntologyComparisonPair` renders changed statements for both the per-file reading view and All changes as one bulleted item whose Current and Before rows meet without a gap (owner correction 2026-10-30: an earlier version gave each row its own bullet); All changes passes each file's workspace, repository and path to statement Markdown. Tests: `OntologyComparisonPair.test.tsx`, `OntologyComparison.test.tsx`, `e2e/mock-preview/file-editor.spec.ts`.
- The [owner request of 2026-11-03](../../../chat_intent/saehan-ontology-diff-improvement/request.md) lifts the earlier word-level highlighting deferral into [change highlights](#g5.documents.ontology.change-highlights).

### g5.documents.ontology.change-highlights

- human-verified
- Expands: [Ontology](#g5.documents.ontology).
- test-coverage
- The owner chose the agent's option C on [2026-11-03](../../../chat_intent/saehan-ontology-diff-improvement/request.md): align sentences, then highlight changed words only inside sentences that changed, and fall back to the whole-statement pair when a statement changed too much, without a change summary. Rendered-text comparison, the CSS Custom Highlight mechanism, the 2,000-word cap and the fallback thresholds are agent design in the [plan](../../../pr_ontology_diff_design/saehan-ontology-diff-improvement/ontology-plan.md), checked against this repository's recent statement edits. The suggestion-preview fallback and the comparison work bound are agent design from [review](../../../pr_ontology_diff_design/saehan-ontology-diff-improvement/ontology-plan.md#review--2026-11-04): the preview hides the text it covers, which would otherwise read as removed, and the word cap alone does not bound punctuation-heavy text.
- The owner chose color-only marks on 2026-11-03 because strikethrough is hard to read; the marks therefore rely on color, with the row labels telling added from removed.
- The owner chose the shorter Before version on [2026-11-04](../../../chat_intent/saehan-ontology-diff-improvement/request.md#human-instructions--2026-11-04), after UI review feedback that the Before row costs space it does not earn when only additions are marked. Both rows keep their backgrounds. A single merged row was declined: without strikethrough it reads poorly once text is removed, and it would put removed text into the live statement. The excerpt's plain-text rendering is agent design in the [plan](../../../pr_ontology_diff_design/saehan-ontology-diff-improvement/ontology-plan.md#shorter-before-version--2026-11-04).
- The owner moved the controls to the bottom of the label column on [2026-11-04](../../../chat_intent/saehan-ontology-diff-improvement/request.md#human-instructions--toggle-placement-2026-11-04), after the agent's UI review found that Hide before appeared on another row than Show before. The agent first kept Show less's position on screen when Before was shortened; after trying it in the preview the owner [chose](../../../chat_intent/saehan-ontology-diff-improvement/request.md#human-instructions--collapse-keeps-the-statement-2026-11-04) to keep the statement still instead, because moving it felt wrong. Accepted risk: after Show less on a Before row long enough that its top had scrolled away, the pair can end up above the view.
- Known limit: a link-target, formatting or wrapping change is not highlighted, so a pair changed only that way looks unchanged. Diff Graph and the rebase/merge preview keep their line diffs.
- Implemented in `lib/ontologyChangeHighlights.ts` and `useChangeHighlights` (applied by `OntologyComparisonPair`). Tests: `ontologyChangeHighlights.test.ts`, `useChangeHighlights.test.tsx`, and the real-Chromium journey in `e2e/mock-preview/file-editor.spec.ts` (fixture `?ontology&changed&sentences`). See [evidence](../../../pr_ontology_diff_design/saehan-ontology-diff-improvement/ontology-plan.md#implementation-and-verification--2026-11-03).

### g5.documents.ontology.all-changes

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Selected continuous read-only overview](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#all-ontology-changes--2026-10-16).
- Capture, limits and verification: [existing plan](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#all-ontology-changes--2026-10-16).
- [Issue #666 owner decision 16](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20) adds opening at a named file, including deleted files.

### g5.documents.ontology.comparison-baseline

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [All changes baseline decision](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#core-statement-comparison--2026-10-15); capture and refresh details are agent design.
- Implemented through the shared All changes selection owner and bounded immutable reads. API tests cover divergent histories, changed parent revisions, repository isolation, denied/unavailable history and size refusal; refresh tests cover source events and stale responses. See [scoped implementation evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#comparison-implementation-and-verification--2026-10-15).

### g5.documents.ontology.comparison-matching

- agent
- Expands: [Ontology](#g5.documents.ontology).
- [Core comparison decisions](../../../chat_intent/saehan-499-ontology-editor/core-ontology.md#core-statement-comparison--2026-10-15), including no metadata-only flags and deferred prose comparison.
- Implemented; parser/component checks cover body-only matching, malformed structure, limits, historical isolation and own-version metadata. Real browser Save/Undo/conflict checks preserve the current draft. See [scoped implementation evidence](../../../pr_ontology_diff_design/saehan-499-ontology-editor/ontology-plan.md#comparison-implementation-and-verification--2026-10-15).
- Issue #666 applies this rule unchanged to Diff Graph and the rebase/merge preview; the owner [deferred move tracking](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--move-tracking-and-comparison-owner-2026-10-20).
- Alias anchors are identity lines: [Verification display decisions, 2026-10-31](../../../chat_intent/ontology-formal-verification/verification-display.md), decision 13.

### g5.documents.ontology.comparison-owner

- human-provided
- Expands: [Ontology](#g5.documents.ontology).
- [Move tracking and comparison owner decision](../../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--move-tracking-and-comparison-owner-2026-10-20): with same-file pairing, the owner chose two implementations held equal by a shared corpus over one server implementation, avoiding server work on every edit. This relaxes the earlier "one statement parser and pairing rule" note.
- Implemented: `g5_semantic::ontology_comparison` and `frontend/src/lib/ontologyComparison.ts` both pass `tests/fixtures/ontology-comparison` (24 cases), with targeted mutation controls killed. The Rust parser was aligned with the editor on nested boundaries, fences and heading anchors. See [evidence](../../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).
