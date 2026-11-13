# Diff Graph

<a id="g5.diff-graph"></a>
[I · g5.diff-graph]

Diff Graph shows what a workspace changed relative to its parent, as a graph of changed artifacts (intents, ontology and source code) and the authored links between them. Statement identity, comparison and the Human rule come from the [Core Ontology comparison contracts](documents/ontology.g5.md#g5.documents.ontology.comparison-owner); this page owns only what the change graph adds.

## Story and surfaces

<a id="g5.diff-graph.story"></a>
[I · g5.diff-graph.story]

A workspace member sees how changed ontology statements link to other artifacts and opens each change where it can be compared. One change graph serves the Diff Graph tab, the Changed nodes list in Review changes and the rebase/merge preview; each adds only its own presentation.

## Classification

<a id="g5.diff-graph.columns"></a>
[D · g5.diff-graph.columns]

Lay changed files out in three columns, classifying each path in this order:

- **Ontology:** [ontology files](documents/ontology.g5.md#g5.documents.ontology.current-format) in any workspace repository, as one column grouped by folder.
- **Intents → Chats:** every `.md` file at any depth under `.g5/chat_intent/`.
- **Intents → Documents:** design plans at `.g5/pr_ontology_diff_design/<topic>/ontology-plan.md`, and files under `docs/` in the docs repository except question sets (`docs/<workspace-id>/questions/*.g5q.json`).
- **Intents → Other:** question sets, any other path containing a `.g5` folder (including other files under `.g5/chat_intent/` and `.g5/ontology/`), and docs-repository paths outside `docs/`.
- **Source Code:** everything else.

Legacy `.semlift` layouts, `*.semlift.md` files, `@ref` text and the `intent_record`/`intent_grounding` ledgers have no special meaning; they are ordinary files under these rules.

<a id="g5.diff-graph.modes"></a>
[D · g5.diff-graph.modes]

All changes compares each repository with its merge base with the parent, under the [comparison baseline](documents/ontology.g5.md#g5.documents.ontology.comparison-baseline) rules. The rebase/merge preview compares committed heads, because that is what a merge applies.

- Current content is the reader's saved workspace content for every file, including uncommitted edits, read as the [owner](workspaces.g5.md#g5.workspace.owner-read) or a [non-owner](workspaces.g5.md#g5.workspace.durable-read) sees files. Unsaved browser buffers are excluded. Each node says whether its change is committed, uncommitted or both.
- If a repository's live state cannot be read completely, for example while it has unpublished local commits, the graph says that repository shows committed content only, and why.
- Uncommitted only is offered only once uncommitted changes are reported. Before that, the API refuses the mode as unavailable instead of returning an empty graph, and consumers show their unavailable state.
- Whole-graph and per-column empty messages keep their wording.

## Ontology nodes

<a id="g5.diff-graph.ontology-nodes"></a>
[D · g5.diff-graph.ontology-nodes]

Each shown ontology file is one node. Its details list its statement changes (Added, Modified and Deleted), paired within the file by the [matching rule](documents/ontology.g5.md#g5.documents.ontology.comparison-matching).

- A file is shown when it has a statement change, a comparison warning or other changed text. Metadata-only, heading and changes to prose outside statements add one file-level Other changes note with their remaining diff and never flag a statement.
- Moves are not tracked: a renamed file appears as a deleted file and an added file, and a statement moved between files as Deleted in one and Added in the other.
- A file version that cannot be compared reliably shows a Cannot compare warning and its file diff. None of its statements is classified, and the rest of the graph is unaffected.

## Relationships

<a id="g5.diff-graph.relationships"></a>
[D · g5.diff-graph.relationships]

Draw the authored Markdown links of changed statements (added, modified or deleted) as typed relationships, plus any link added to or removed from an unchanged statement. Each pair of file nodes gets one summary edge, with one evidence row per drawn link marked as added, removed or unchanged. A link means that a statement cites the target, not that either depends on the other.

- An unchanged link of an unchanged statement is not drawn, so it creates no context node.
- Links within one file and links with no [kind](#g5.diff-graph.relationships.kinds) are not drawn. An unchanged target appears as a context node marked unchanged.
- Kinds look different and are named in details. Edges are not clickable; selecting an evidence row highlights its edge.
- Relative links resolve in their own repository, against the version that contains them: current text against current files, deleted text against the baseline.
- `g5://file` links resolve in another repository of the same workspace. Absolute URLs, including GitHub permalinks, are external citations.

<a id="g5.diff-graph.relationships.kinds"></a>
[D · g5.diff-graph.relationships.kinds]

A link's kind follows from its syntax and target:

- **Expands:** a formal child's `Expands` parent link.
- **Reference:** any other link to an ontology statement or to a whole ontology file.
- **Provenance:** a link to a chat intent record or a design plan.
- **Implementation:** a link to source code.
- **Project document:** a link to a Project Docs file.

<a id="g5.diff-graph.missing-targets"></a>
[D · g5.diff-graph.missing-targets]

Resolve each link by its exact path and anchor; never follow a stable ID to another location, consistent with [moves](documents/ontology.g5.md#g5.documents.ontology.moves). An ambiguous target never links to a guessed statement, and unresolved links never fail the graph.

- If the target statement or file was deleted in this comparison, including one that moved, the link points at the deleted node, marked as a deleted target.
- Any other unresolved link, such as a typo or a target that never existed, is a missing target. It is drawn as a dangling Missing target marker only when the link itself was added or edited in this comparison; otherwise it is neither drawn nor reported.

## Human intent

<a id="g5.diff-graph.human-lens"></a>
[D · g5.diff-graph.human-lens]

The Human intent lens keeps ontology files with at least one changed [human statement](documents/ontology.g5.md#g5.documents.ontology.human-statements), judged on the version shown for that change, and lists only those statements in details. Links and context nodes of kept files remain.

- No other artifact is human-attributed, so the lens hides other changed artifacts.
- The exception is a changed artifact that a kept file links to: it stays as a context node showing its own change state.

## Navigation

<a id="g5.diff-graph.navigation"></a>
[D · g5.diff-graph.navigation]

Selecting a node opens its details panel. Other artifacts keep their actions, and the selected node, mode and lens stay out of the page address.

- For an ontology file, Open document opens the ontology editor at the file's exact repository and path with Compare on. Each changed-statement row opens the editor with Compare on at that statement.
- A deleted statement opens at its read-only Before row; a deleted file opens All changes at that file.
- Ontology nodes offer no Source Control diff, because its baseline differs.
- The rebase/merge preview shows statement changes in its details panel without navigation on either side.

## Rebase and merge preview

<a id="g5.diff-graph.rebase-conflicts"></a>
[D · g5.diff-graph.rebase-conflicts]

The preview's always-visible Ontology layer shows one Source and one Target ontology lane. If both sides change the same statement ID in the same file, that is a same-statement conflict; if both sides change the same file without a shared statement, that is a same-file conflict. Other layers keep their behavior.

## Limits

<a id="g5.diff-graph.limits"></a>
[D · g5.diff-graph.limits]

Diff Graph enforces the bounds listed here. Exceeding a read limit refuses the graph as too large rather than omitting files, and relationship budgets truncate visibly.

- 500 changed files and 100 MiB of compared input.
- 256 context nodes, 1,024 edges and 4,096 evidence rows.
- Detail diff budgets.
- The ontology comparison reads every ontology file in both versions under the [All changes](documents/ontology.g5.md#g5.documents.ontology.all-changes) limits: 500 files, 100 MiB, 10 MiB per file and 5,000 statements per file.
- Saved edits: 500 live ontology pages and 500 changed paths, checked from the listings before any read.
- Links look up at most 1,024 distinct unchanged target files. Further links are skipped with a visible budget diagnostic, never reported as missing.

## Metadata

### g5.diff-graph

- agent
- Formerly the page's introduction; made a statement to define the `g5.diff-graph` level ([writing principles](https://github.com/G5Labs/G5-Lean/issues/1119)).
- Children: [Story](#g5.diff-graph.story), [Columns](#g5.diff-graph.columns), [Modes](#g5.diff-graph.modes), [Ontology nodes](#g5.diff-graph.ontology-nodes), [Relationships](#g5.diff-graph.relationships), [Missing targets](#g5.diff-graph.missing-targets), [Human lens](#g5.diff-graph.human-lens), [Navigation](#g5.diff-graph.navigation), [Rebase conflicts](#g5.diff-graph.rebase-conflicts), [Limits](#g5.diff-graph.limits).

### g5.diff-graph.story

- human-provided
- Expands: [Diff Graph](#g5.diff-graph).
- [Issue #666 and surfaces decision](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20) (decision 1).
- Design: [issue #666 plan](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md).

### g5.diff-graph.columns

- human-provided, agent
- Expands: [Diff Graph](#g5.diff-graph).
- [Owner decisions 2–4 and 18](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20). Rule order, the source-code fallback and the outside-`docs/` rule keep the current classifier.
- Implemented in `classify_path` (`backend/g5-api-core/src/services/git/source_control_change_graph_service.rs`); unit and HTTP integration tests cover every rule, including legacy layouts as ordinary files. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.diff-graph.modes

- human-provided, human-verified
- Expands: [Diff Graph](#g5.diff-graph).
- [Owner decisions 9, 10 and 17](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20), extended to every file by the [uncommitted scope decision](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decision--uncommitted-scope-2026-10-20). The rebase/merge preview staying on committed heads is [owner-confirmed](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--navigation-and-scope-2026-10-20), as is the per-repository fallback.
- Lineage: before `09f11e34` the graph layered server-side CRDT drafts over committed heads; the cutover left that overlay input empty.
- Implemented: `routes/change_graph_live.rs` captures saved content; the service layers it per repository and refuses `uncommitted`. HTTP integration tests use the runtime double for owner, reader, stopped-VM, unpublished-commit, behind, failed-report, unreadable and vanished cases; a real-stack browser journey checks the live-content line. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.diff-graph.ontology-nodes

- human-provided
- Expands: [Diff Graph](#g5.diff-graph).
- [Owner decisions 5, 6 and 8](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20); moves follow the later [move tracking deferral](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--move-tracking-and-comparison-owner-2026-10-20), which supersedes decision 15.
- Implemented with `g5_semantic::ontology_comparison`, which passes the shared corpus; HTTP tests cover statement states, metadata-only and refused files and renames as delete plus add; a browser journey checks the cards and notices. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.diff-graph.relationships

- human-provided, agent
- Expands: [Diff Graph](#g5.diff-graph).
- [Owner decisions 5, 11, 12, 14 and 16](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20); links come only from changed statements and changed links by the [link scope decision](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decision--link-scope-2026-10-21). Kind assignment by syntax and target and the per-link evidence states are agent design.
- Implemented in `statement_links` and `resolve_link`; a unit test covers the link scope; HTTP tests cover each kind, same-file and external links, `g5://file` and context nodes; a browser journey checks the legend and edge kinds. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).
- Children: [Kinds](#g5.diff-graph.relationships.kinds).

### g5.diff-graph.relationships.kinds

- agent
- Expands: [Relationships](#g5.diff-graph.relationships).
- Kind assignment by syntax and target is agent design under [owner decisions 5, 11, 12, 14 and 16](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20). Split from `g5.diff-graph.relationships` to keep each statement within three sentences.

### g5.diff-graph.missing-targets

- human-verified
- Expands: [Diff Graph](#g5.diff-graph).
- Combines [owner decision 13](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20) with the [move tracking deferral](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--move-tracking-and-comparison-owner-2026-10-20): a link to a moved target points at the deleted original, never at the new location.
- Implemented; HTTP tests cover a deleted target, an edited missing link and an unchanged broken link; unit tests cover the exactly-one-anchor rule. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.diff-graph.human-lens

- human-provided, agent
- Expands: [Diff Graph](#g5.diff-graph).
- [Owner decisions 7 and 18](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20). Hiding non-ontology artifacts follows from removing the ledgers. Keeping a linked changed artifact as context is an implementation discovery (agent): dropping it would also drop the kept file's link.
- Implemented in `select_graph`; unit and HTTP tests cover kept files, hidden artifacts and linked changed context; a browser journey checks the lens. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.diff-graph.navigation

- human-provided
- Expands: [Diff Graph](#g5.diff-graph).
- [Owner decision 16](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--2026-10-20); row-level opening and preview navigation on both sides: [navigation and scope decisions](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--navigation-and-scope-2026-10-20).
- Implemented through `onOpenOntology` in `WorkspacePage.tsx`, the editor's deleted-statement target and the All changes file target; component tests and a browser journey open a deleted statement at its Before row. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.diff-graph.rebase-conflicts

- human-verified, agent
- Expands: [Diff Graph](#g5.diff-graph).
- Follows the single-column decision (owner decision 3) and same-file pairing ([move tracking deferral](../../chat_intent/issue-666-fix-diff-graph-to-support-current-ontology/decisions.md#human-decisions--move-tracking-and-comparison-owner-2026-10-20)); lane and conflict rules are agent design.
- Implemented in `classify_rebase_conflicts` and the preview's two lanes; an HTTP test covers same-statement and same-file conflicts. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).

### g5.diff-graph.limits

- agent
- Expands: [Diff Graph](#g5.diff-graph).
- Existing constants in `source_control_change_graph_service.rs` and `backend/g5-api-routes/src/routes/ontology_changes.rs`.
- Live-read and link-target bounds are implementation discoveries recorded in the plan; unit tests cover each budget boundary. See [evidence](../../pr_ontology_diff_design/issue-666-fix-diff-graph-to-support-current-ontology/ontology-plan.md#implementation-and-verification--2026-10-20).
