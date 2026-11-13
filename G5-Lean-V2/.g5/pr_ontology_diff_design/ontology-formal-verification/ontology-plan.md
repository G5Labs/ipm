# Ontology verification display

Design for the Core Ontology viewer's verification display and the related harness changes. Owner decisions are in the [decision record](../../chat_intent/ontology-formal-verification/verification-display.md). The reviewed design, with its pilot measurements and question set, is the Project Doc `ontology-verification-display-design.md` of workspace `ziniuw/ontology-formal-verification`. Governing statements:

- [`g5.documents.ontology.disclosure`](../../ontology/features/documents.g5.md#g5.documents.ontology.disclosure)
- [`g5.documents.ontology.verification`](../../ontology/features/documents.g5.md#g5.documents.ontology.verification) and its children
- the amended [`current-format`](../../ontology/features/documents.g5.md#g5.documents.ontology.current-format), [`presentation`](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation), [`links`](../../ontology/features/documents.g5.md#g5.documents.ontology.links) and [`comparison-matching`](../../ontology/features/documents.g5.md#g5.documents.ontology.comparison-matching)
- [`g5.harness.verification-artifacts`](../../ontology/features/ontology_and_harness.g5.md#g5.harness.verification-artifacts)

## Viewer

### Statement tree and folds

Preview of ontology files only:

- **Children.** A statement is the child of the nearest earlier statement whose ID it extends with `.`, when only that statement's descendants and no section heading come between them.
- **Lead and fold.** The lead is a statement's first paragraph; question cards, per-statement warnings and badges stay with it. The rest of a top-level statement, plus its children, is its fold.
- **Markers.** A fold has a triangle button; a statement with nothing to fold has a dot. Children are sub-bullets without folds.
- **Hidden content.** Folds start closed. Hidden content stays in the page (`hidden`), so anchors, line lookups and selection mapping keep working.

What opens folds:

- navigation to a statement, heading or line inside a fold;
- comment reveal;
- Compare changes in a statement or its descendants;
- filter matches;
- review suggestions anchored inside.

A reader's choice holds until a new reason opens the fold. Margin cards whose text is hidden are placed level with the statement holding it.

**Owners.**

- `currentOntology.ts` decides the tree, the lead block and each statement's verification tag and linked state machines.
- `documentMarkup.ts` nests child statements and splits the fold in the rendered tree.
- `DocumentPreview.tsx` owns the reasons folds open. Fold and panel choices live in a small store (`statementState.ts`) that each statement subscribes to, so a click re-renders only that statement, not the whole document.
- `SemanticRecordFrame.tsx` renders the marker, status button and panel slot; frames outside ontology Preview keep their markup.

### Verification status

- **Tag.** A metadata bullet consisting of exactly one verification tag sets the status; the strict tag rule applies. The Human rule, and its server copy, are unchanged.
- **Design verified** also requires evidence: a linked concern folder with `manifest.json` and a run index row for it. Missing or unreadable evidence shows Unverified, with Retry for a failed read.
- **Pending.** "Checking…" is shown until the evidence is known.
- **Roll-up.** A statement shows Design verified through any Design verified descendant, unless its own status is Implementation verification; status never passes down.

**Owners.** `frontend/src/lib/verificationArtifacts.ts` is the only code that interprets artifact formats and paths, following [the reader contract](../../../tools/tlc/verification-artifact-reader.md). `useVerificationEvidence.ts` reads each linked manifest and the run index directly, in one query per file, never listing folders: Files hides `.g5/TLA_verification`, and its tree would not fit the pilot repository's artifacts. Reads are cached for 30 seconds and read again when the file changes. The run index, the slowest read (3.5 MB in the pilot repository), starts beside the manifests rather than after them, for each link's root as written.

### Verification panel

Unverified and Checking… open nothing. For a design-verified tag, each linked state machine, in first-linked order:

- **Shown at once:** "[title]: What this state machine checks" and the manifest's question.
- **Folded:**
  - `BINDINGS.md`, with links resolved from its own folder;
  - the declared runs' latest executions, with their outcome, kind, variant, expectation, states and receipt link;
  - earlier and retired runs;
  - the files named by the manifest.

Test Coverage, Manual checking and review and Implementation verification show the statement's metadata entry without its bare tag bullets and `Expands:`/`Children:` lines. Panel content is excluded from selection comments and double-click editing. There is no status selector and no bugs, issues or simplifications.

### Links

- **Relative links** try, in order:
  1. the authored file-relative path (skipped when it climbs above the root);
  2. the same with leading `./`/`../` removed, from the repository root;
  3. the same from the `.g5` folder.

  The first that exists opens, checked by reading it; a read that cannot tell opens that path. Otherwise a message names the paths tried. Only ontology files resolve links this way; other documents keep their links as authored.
- **Line fragments** open at the line, through the existing "open file at line" call.
- **Unencoded spaces.** Space-containing destinations render as links.
- **Other files' content** carries its own link base.
- **Alias anchors** resolve to their statement.

### Alias anchors

Consecutive anchor-only lines directly above a statement's own anchor are its aliases:

- They are identity lines: hidden, outside the previous statement's body, and refused when repeating any statement's ID.
- A statement's source starts at its first alias.

Both statement parsers implement this: `current_ontology_records` in `g5-contracts`, the comparison in `g5-semantic` and the Diff Graph `FragmentIndex` on the server; `currentRecordSpans` in the browser. The shared corpus in `tests/fixtures/ontology-comparison/` and `tests/fixtures/current-ontology.json` holds them equal.

## Harness

- **Artifact guide.** [tla-verification.md](../../../.agents/guidance/TLC/tla-verification.md) stays a separate guide, linked from uplift, downlift, uplift's evidence companion and planning's model-checking reference. Since the [findings decisions](../../chat_intent/ontology-formal-verification/findings-triage.md) (decision 17) it is `main`'s short storage note again; the viewer's field rules and the helper live in the [review guide](#findings-triage).
- **Helper script.** [`link_verification.py`](../../../.agents/guidance/scripts/link_verification.py):
  - copies manifest associations into metadata links on a `State machines:` line;
  - tags linked statements that have evidence `design-verified`, by the viewer's test, replacing a single `test-coverage` or `manual-check-review` but never `implementation-verified`.

  It never removes links or tags, edits prose or judges outcomes. It reports unknown and duplicate IDs, conflicting tags and unrecognized tag-like bullets, and leaves those entries as they are.
- **Format guide.** [ontology-format.md](../../../.agents/guidance/ontology-format.md) adds:
  - child placement;
  - alias anchors;
  - bare tag bullets;
  - at most one verification tag;
  - the meaning of an association.

  It removes `future`.

## Assumptions and acceptance

- **Accepted by the owner:**
  - Tags can drift from evidence; only evidence existence is checked.
  - Closed folds hide children's questions and warnings.
  - Under the strict rule, the pilot repository's `- agent.` provenance bullets are not recognized.
- **Capacity.** The run index must fit the 32 MiB read limit. Evidence takes one read per linked state machine (about 30 for the pilot repository's largest page) plus the index.
- **Files visibility, decided** (findings decision 21). Files does not list `.g5/TLA_verification`; the panel opens its files directly. Listing it would push most of a repository the size of the pilot out of the Files tree, which loads at most 10,000 entries.
- **Known gaps.**
  - Link existence is checked by reading the target. Since 2 November a successful check also fills the file view's cache for that file when it is empty, so the opened file shows that read rather than reading again; a newer cached read is never replaced, and a failed check stays out of other views.
  - Only the artifact guide's run format is read.
  - The full-stack flow test runs only in CI.
  - In Compare, a changed statement's Before row follows its opened fold.
  - Already on `main` and unchanged:
    - an ontology file with a byte-order mark loses its leads in Preview;
    - the parsers count anchors in list items differently;
    - the Diff Graph resolves links by exact path.
- **Acceptance:**
  - Unit, component and corpus tests for every rule above.
  - Browser checks on a synthetic file set shaped like an uplift's.
  - A full-stack flow test.
  - A manual preview on a subset of the pilot repository compared with the owner's mock.

## Findings triage

Owner decisions: [findings decisions](../../chat_intent/ontology-formal-verification/findings-triage.md). Reviewed design: the Project Doc `findings-triage-design.md`. Governing statements: [`g5.documents.findings`](../../ontology/features/documents.g5.md#g5.documents.findings) and its children, the amended [`verification.details`](../../ontology/features/documents.g5.md#g5.documents.ontology.verification.details) and [`g5.harness.verification-artifacts`](../../ontology/features/ontology_and_harness.g5.md#g5.harness.verification-artifacts). Relied on, unchanged: [issue creation](../../ontology/features/issues.g5.md#g5.external-issues.creation) with its [admission](../../ontology/features/issues.g5.md#g5.external-issues.creation.admission) and [recovery](../../ontology/features/issues.g5.md#g5.external-issues.creation.recovery), [native writes](../../ontology/features/issues.g5.md#g5.issue.native.writes), [edit](../../ontology/features/issues.g5.md#g5.external-issues.edit), [transition](../../ontology/features/issues.g5.md#g5.external-issues.transition), [VM flush](../../ontology/software_architecture/workspace_vm.g5.md#g5.vm.flush) and [chat actions](../../ontology/features/ui.g5.md#g5.workbench.chat-actions).

### Owners

- **The format** has one reader per language: [`check_findings.py`](../../../.agents/guidance/scripts/check_findings.py) for agents and `frontend/src/lib/findings.ts` for the viewer. They share the fixtures in `tests/fixtures/findings/`, so both read entries, severities, decisions and problems the same way.
- **The decision record** is the entry's status line in the repository file. The viewer rewrites only the decision after ` · Decision: `, through the file view's live write with commit and push off. The write is checked against the file's version, and the VM's flush keeps it until the workspace's next commit.
- **The issue** belongs to the organization's creation tracker. The viewer uses the shared routes: create, with check-only under a per-entry key `finding:<repository id>:<path>#<id>`; resolve; edit title; and state. It holds no state of its own.
- **Presentation.** A rehype step (`findingsMarkup.ts`) wraps each entry and replaces its status line with a control row. `FindingsControls.tsx` renders the row, the severity filter and the decision sequence. The Core Ontology panel reads the two paths directly, as the evidence reader does, because Files hides the folder.

### Sequence of a press

1. **Gate.** The owner of a running, writable workspace, with no unsaved draft of the file.
2. **Tracker.**
   - Need fix or Defer: if no issue is recorded, look the key up and create the issue only if none exists. If the entry already records an issue, reopen it when closed and retitle it when its prefix differs.
   - Irrelevant: close a recorded open issue.
   - An untrusted GitHub destination opens the shared Create issue dialog, carrying the same key, for its payload review.
3. **Record.** Rewrite the line in the current content and write it live.
   - A refused write keeps the issue and shows Retry, which rewrites the line in the content then current.
   - A lost record is recovered by the key lookup on the next press: exactly once for native issues, and within GitHub's recent-issue lookup otherwise.

### Choices and reasons

- **No question sets.** They live in one workspace's Project Docs, and decision 1 makes the record repository-owned.
- **No commit per decision** (decision 22). The issue exists at once, and the line rides with the next Save changes or PR update.
- **Filter, not reordering.** The checker keeps the file most severe first, so the reader only hides severities.
- **Actionable means trackable** (decision 23). Defects, findings that need a product or design decision first, and simplifications still proposed get a decision line; severity never decides it. Only the guide defines this; the viewer and checker read the decision line.
- **No backend change.** Every tracker operation already exists for the issue page, and the live write exists for Preview writes.

### Assumptions and acceptance

- **Assumption: the VM commits the folder.** Inside the workspace VM, `.g5/TLA_verification/` is committed and flushed like ordinary content, since Git there excludes only G5's runtime folders. G5's own repository keeps just the two files tracked.
- **Assumption: GitHub retitles may be refused.** Retitling a GitHub issue sends the title the viewer read, and is refused if the title has changed since.
- **Assumption: creation keys are per person.** Two owners deciding the same entry in different workspaces before either decision is merged file separate issues; the decision line, merged through Git, normally prevents that.
- **Known limits:**
  - A review suggestion that replaces an entry's status line hides that entry's control row until the suggestion is resolved.
  - Issue links point into the deciding workspace's route; from a personal workspace that is `/<org>/me/…`, which opens the reader's own personal workspace.
- **Acceptance:**
  - checker and parser tests on the shared fixtures;
  - component tests for every row of the decision table, the filter, the read-only reasons, the panel section and the Core Ontology entries;
  - a browser check on the fixture page;
  - the pilot dry run checked clean and rendered in the local preview, with decisions made against its native tracker.

### Preview review amendments (2 November 2026)

The owner's [preview review](../../chat_intent/ontology-formal-verification/findings-triage.md#preview-review-2-november-2026), decisions 24–28, amends [`ontology.presentation`](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation), [`ontology.links`](../../ontology/features/documents.g5.md#g5.documents.ontology.links), [`findings.files`](../../ontology/features/documents.g5.md#g5.documents.findings.files), [`findings.decision`](../../ontology/features/documents.g5.md#g5.documents.findings.decision) and [chat actions](../../ontology/features/ui.g5.md#g5.workbench.chat-actions).

- **Kind is not a tag.** `SemanticRecordFrame` drops the Question badge of a Q statement on every surface; Type in the details still says Q. The format guide now says "at most one provenance tag".
- **Filter and look.** `findingsMarkup.ts` also wraps the description between entries (`g5-findings-context`) and keeps apart the last heading before a run of entries (`g5-findings-heading`, with its section's severities). The filter hides the description with one attribute, and only those headings read the hidden severities, so a toggle still re-renders no entry. The cards, decision colours and the page-level reason are CSS and one note.
- **Discuss beside the document.** `handleDiscussInChat` seeds through the existing seed, then opens the existing right-edge panel as the corner Chat button does. Question cards' Discuss and a finding's chat action use it; other seeds and phones keep the center.
- **Back.** `NavigationOrigin` now names the file it opened, and only that file shows Back. Origins come from the existing `onNavigate` (links through the ontology reference popover) and from the file view's other opens of another file, such as a source line (`onOpenWorkspaceFile`, which now carries the statement the link was in). The one Back button of `CodeEditor` also shows over the rendered view. It stays single-step: Back uses up the origin.
- **Tests:** `DocumentPreview.findings.test.tsx` (filter, headings, notice, card), `CodeEditorTraceNavigation.test.tsx`, `CurrentOntologyPreview.test.tsx`, and `WorkspacePageDocumentNavigation.test.tsx` for Back and the side-panel chat through the whole page.

### Evidence and the severity dot (3 November 2026)

The owner's [gaps against the mock](../../chat_intent/ontology-formal-verification/findings-triage.md#gaps-against-the-mock-3-november-2026), decisions 29–33, amend [`verification.status`](../../ontology/features/documents.g5.md#g5.documents.ontology.verification.status) and [`findings.files`](../../ontology/features/documents.g5.md#g5.documents.findings.files).

- **Dot.** `findingsDot` (`lib/findings.ts`) picks the most severe linked finding that is Open, Need fix or Defer, if Critical, High or Medium. `DocumentPreview` reads each linked findings file once through `useFirstVerificationTexts`, which shares the cache entries of the panel's `useFirstVerificationText`, and counts a statement's descendants' links. `VerificationStatusButton` draws the dot and names it in its label.
- **Format.** Entries carry `Found by:`, `Reproduced by:` and `Comparison:` lines after the status line ([review guide](../../../.agents/guidance/TLC/verification-review.md#findings)). Parsing stays line-based and shared: the viewer's parser and `check_findings.py` read entries as before, and only the checker validates the new lines (required on Open entries, receipts exist, no `Sources:` on entries that can change), so decided entries are never asked to change.
- **Rendering.** `findingsMarkup.ts` turns those paragraphs into `g5-finding-evidence` elements, splitting lines written without a blank line, and adds an empty `Reproduced by` to an actionable bug that has none. `FindingEvidence` (`FindingsControls.tsx`) looks the linked receipts up in the run index of the file's folder (`useRunIndex`, `receiptRun`) and renders them with the panel's `RunTable` in a compact form; `compareStates` (`verificationArtifacts.ts`) decides what a comparison may say, in the mock's words. A bug's Found by table stays closed under a summary counting its runs and state machines until opened, as the mock keeps model evidence: laying out the pilot repository's 177 tables up front added about 2 s to opening the page.
- **Pilot data.** The local copy was converted by a script that keeps no customer text: run receipts from each entry's existing run list, reproductions from linked tests and probes and from the mock's owner-confirmed reproductions, comparison pairs from the mock's curated before/after runs, matched in the run index by state machine, label and state count. All 72 actionable simplifications show the mock's own result: the 19 like-for-like pairs the same percentage (6 fewer, the rest 0% or more), the others no comparable reduction.
- **Tests:** dot and evidence component tests (`DocumentPreview.verification.test.tsx`, `DocumentPreview.findings.test.tsx`), checker tests (21), the browser findings spec, and a check on the pilot data in the preview.

### Preview check amendments (3 November 2026)

The owner's [preview check](../../chat_intent/ontology-formal-verification/findings-triage.md#preview-check-3-november-2026), decisions 34–36, amends [`findings.files`](../../ontology/features/documents.g5.md#g5.documents.findings.files) and both guides.

- **Comparison last.** `findingsMarkup.ts` moves a comparison element to the end of its entry, so the file format is unchanged and older files render the same way.
- **No reproduction.** `FindingEvidence` renders the heading alone with its note in bold, and a visually hidden "None recorded." for screen readers.
- **Material.** [`tla-verification.md`](../../../.agents/guidance/TLC/tla-verification.md) asks producers to record each run's variant and name the as-built and simplified runs of each simplification; [`verification-review.md`](../../../.agents/guidance/TLC/verification-review.md#findings) says how a preparer finds and judges the pair. The viewer does not pair runs.
- **Pilot data, without the mock.** The local copy's comparisons were derived as the review guide now says: the simplified run from the entry's recorded variants (`simplification`, `candidate`, `conditional-candidate`, else `repair` or `intended`, marked not like for like), the as-built run of that state machine from the entry or the run index, and like for like only when the two configurations match apart from TRUE/FALSE design switches and the simplified run checks every property the as-built run checks. Result: 27 like-for-like pairs (20 with both runs passing, so a ratio shows; the mock had 19 ratios), 24 not like for like, 12 with no simplified run and 9 with no comparison. The mock's headline pair keeps its 44.2% fewer explored states.
- **Tests:** `DocumentPreview.findings.test.tsx` (order, empty Reproduced by), the browser findings spec (bold, no text) and the preview check on the pilot data.

### Rebase onto #925 (3 November 2026)

Main gained #925's one-surface reading tools: Compare and the filters in the file row, the reading view's state kept across view switches, changed statements as one comparison pair, selection commenting and no record Comment action. Both sides' intent is kept:

- **Reading tools.** Expand all and Collapse all join Compare and the filters in main's toolbar, which the file row holds ([`editor.modes`](../../ontology/features/documents.g5.md#g5.documents.editor.modes)). The findings severity filter stays above the entries: the owner's file-row decision named Compare and the two filters only, and decisions 25–26 accepted the filter where it is.
- **Kept state.** `statementState` and Expand all live in main's `PreviewReadingState`, so a mode switch keeps folds and open verification details as it keeps Compare and open details.
- **Changed statements.** `OntologyComparisonPair` renders a changed statement and `OntologyBeforeStatement` a deleted one; both take the row's layout class. With disclosure markers the pair draws no bullet of its own: the statement's triangle or dot, or a deleted statement's dot, hangs in the marker column with every other marker, and the pair, change label first, starts where statement text starts, its text where All changes puts it ([`presentation`](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation)). main's layout journey (`file-editor.spec.ts`) now checks these positions in place of the disc bullet.
- **Targets.** Fold reveal and finding anchors join main's once-per-request target resolution (`targetElement`), so a target in a closed fold opens it and is highlighted once.
- **Commenting.** The verification panel, run tables, findings rows and the findings toolbar are generated, so selecting in them offers no Comment ([`comments.identity`](../../ontology/features/documents.g5.md#g5.documents.comments.identity)).
- **Badges.** Main's #925 hides the permanent Human badge; with decision 24 dropping the Question badge, a statement shows only Uncertain and question state outside its details.

## Implementation and verification

Recorded as steps land.

1. **Ontology and harness (2026-10-31).** Statements, decision record, guide, format guide, helper and reader contract.
   - The helper's 12 tests pass, and the checker's existing 30 still pass.
   - On a copy of the pilot ontology and TLA models, the helper added 536 links on 343 statements and tagged 509 statements. It reported the 5 manifest IDs that point at "Moved to" stubs. A second run changed nothing, and the checker found no new issue from the added links.
2. **Alias anchors (2026-11-01).** Both statement parsers, the comparison source span and the Diff Graph fragment index.
   - New corpus cases: an alias added and an alias removed (no change), a deleted statement taking its alias (only it is Deleted), and an alias duplicating another ID (refused).
   - Shared parser fixture: two new cases.
   - `duplicate-anchor` now records two refused statements, because its stray duplicate anchor sits directly above the next statement's anchor and so is that statement's alias. The first refusal message is unchanged.
   - Results: g5-contracts 391 and g5-semantic 230 library tests pass, as do the Diff Graph fragment tests (including a new alias test), the browser corpus and parser tests, and clippy on the three crates.
3. **Statement tree and folds (2026-11-01).**
   - **What changed:**
     - `documentMarkup.ts` nests child records and splits each statement into lead, `g5-details` and `g5-kids`.
     - `DocumentPreview.tsx` owns fold state and Expand all/Collapse all. It opens folds for navigation targets (including aliases), Compare changes, filter matches and suggestions. Its review placement now looks inside folds, and margin anchors inside a closed fold use the statement's row.
     - `SemanticRecordFrame.tsx` renders the marker column and the fold regions.
   - **Pre-existing bug fixed:** Preview dropped statement text written directly under the marker, with no blank line, because it shares a rendered paragraph with the identity lines. That blanked all 608 pilot statements. Only the identity part is now removed, and the rest keeps its own block index and source line. The parsers already treated those lines separately, so no ontology change is needed.
   - **Tests:** new component tests in `DocumentPreview.folds.test.tsx`; the old fixed-bullet assertions were updated to the amended presentation. 125 frontend test files pass (1,696 tests).
   - **Checked in the browser:** the pilot repository's Receiving page on the backend-free test page.
4. **Status, panel and artifact reader (2026-11-01).**
   - **Status rules:** `verificationArtifacts.ts` (index and manifest parsing, matched-expectation rule, latest-run selection) and `verificationStatus.ts` (tag to status, evidence, parent roll-up, Checking…).
   - **Evidence reads:** `useVerificationEvidence.ts`, with one `models/` listing and one index read per root, plus folder and receipt fallbacks. Reads are cached for 30 seconds, so evidence written by the helper or a new run appears soon.
   - **Panel:** `VerificationPanel.tsx` shows the status button, question, bindings with their own link base, runs, earlier and retired runs, and files.
   - **Tests:** unit tests plus `DocumentPreview.verification.test.tsx`, covering evidence found, missing and unreadable; the tag conflict; and the parent through its child, opening it.
   - **Browser test page:** `e2e/browser/ontology-verification.html` serves a synthetic file set shaped like an uplift's without an API.
   - **Checked against the pilot repository:** loaded locally (not committed) with the helper-processed Receiving page and its linked artifacts. 28 statements show Design verified and 1 Unverified, and the Receiving numbering panel shows the saved question, bindings and nine runs as in the owner's mock.
5. **Links (2026-11-01).** `CurrentOntologyReference.tsx` now:
   - resolves relative links in order (as authored, from the repository root, from `.g5`), checking each candidate against a cached listing of its folder;
   - opens `#L<n>` fragments at the line through the existing open-at-line call (previously they reached navigation as statement fragments);
   - resolves links in content rendered from another file from that file (`data-link-base`);
   - reports a link that finds no file with the paths tried.

   A folder listing that fails, or that answers for another folder, counts as unknown and keeps the authored path, so navigation is never blocked. `spacedLinks.ts` renders space-containing destinations in ontology statements and metadata as links, never from code or for schemes. Tests: `CurrentOntologyReference.links.test.tsx` and `spacedLinks.test.tsx`; the editor and files suites pass (607 tests).
6. **End to end (2026-11-01).**
   - **Browser tests:** `e2e/browser/ontology-verification.spec.ts` checks, in real Chromium:
     - folds by mouse and keyboard, Expand all and Collapse all;
     - the 24px triangle and 18px text;
     - statuses, the parent-through-child note and opening the child;
     - question, bindings, runs and receipt opening;
     - `BINDINGS.md` and metadata links opening at their line, with spaces, and from the repository root;
     - neutral markers in dark mode.
   - **Layout fix:** `markdown-layout.spec.ts` showed the new status button narrowing statement prose below its text-measure bound. The statement row now hangs its marker in the left margin and extends its controls past the measure, so prose starts with the text around it and stays within the measure (`g5.documents.text-measure`).
   - **Remaining failures are pre-existing:** the browser suite's other 6 failures (chat and center width tests) fail identically on `main` at 1c8921c63.
7. **Durable verification root (2026-11-01; reverted in step 9).**
   - **Found during the full-stack preview:** `.g5/TLA_verification/` was hidden runtime metadata. The API listing returned nothing for it, Save staging (`add_all`, `add_paths`) skipped it, and exact-tree commits refused it.
   - **Change:** `g5_paths::is_durable_g5_repo_content_path` and `G5_INTERNAL_EXCLUDE_PATTERNS` now include it, with the same component rules as the design root, and `g5.documents.durable-authored-content` is amended to match.
   - **Tests:** the path classification, visibility and Git exclude-agreement tests cover the new root, nested `.g5`/`.git` sidecars and look-alike folders. g5-contracts (391) and g5-scm (257) library tests and clippy pass.
8. **Final verification (2026-11-01).**
   - **Real G5 UI:** checked on the local full-stack preview, using a workspace seeded with the pilot repository's helper-processed Receiving page and its artifacts (not committed). The page showed 28 Design verified and 1 Unverified, as with the test page. The panel showed the saved question, bindings and runs, and a metadata `#L` link opened the knowledge article at its line.
   - **Frontend:** the full unit suite passes (366 files, 5,610 tests). Type checking, lint and formatting pass. The lint warnings left in changed files were already there before this work, except the Fast Refresh notice that every browser test page gets. The browser spec's untyped `window` reads are now typed.
   - **Not run locally:** the flow test `e2e/flows/ontology-verification.spec.ts`. On the local preview, its workspace stayed `queued_for_capacity` for the whole 180-second wait, because this VM provides no workspace capacity. CI's flow lane runs it.
9. **Review and fixes (2026-11-01).** Four independent reviews covered regressions and scope, the owner's decisions and the ontology, the backend and helper, and the new modules.
   - **Durable root reverted.** Step 7's premise was wrong: production Save (`git add --all`) already commits `.g5/TLA_verification` unless it is gitignored, and the file API reads it. Making it visible would have pushed most of a repository the size of the pilot (about 56,000 artifact entries) out of the Files tree, which loads at most 10,000. The backend and `durable-authored-content` are back to `main`; the viewer reads linked manifests and the run index directly, in one query per file.
   - **Scope narrowed.**
     - The new link resolution, existence check and line opening apply to ontology files only; other documents keep their links as authored.
     - Existence is checked by reading the target, since listings omit folders hidden from Files, such as `.g5/chat_intent`. The check uses its own cache, so a failed check never becomes the reference preview's error.
     - Frames outside ontology Preview keep their markup.
   - **Behaviour fixes.**
     - The panel shows linked state machines whatever the tag.
     - Retry re-reads failed evidence, and stale evidence is re-read when the file changes.
     - Receipts no longer count as runs; the helper and the viewer now share one test.
     - Outcomes are judged against the row's expectation, else the manifest's, and a leading byte-order mark is ignored.
     - A statement with no body keeps its frame.
     - Badges stay beside the lead.
     - A new reason (Compare, a filter, a suggestion) reopens a fold the reader had closed.
     - Links in code examples are not state machine links.
     - Fallback paths resolve `..` and keep the explicit errors for query strings and backslashes.
     - An alias repeating a heading-owned statement's ID is refused by both parsers; a new shared fixture covers it. The browser's alias resolution uses the record parser's strict rule.
   - **Helper.**
     - It encodes link destinations.
     - It leaves conflicting and unrecognized tags as they are.
     - It ignores code.
     - It accepts links written from the repository root or `.g5`.
     - It keeps each line's ending, treats non-UTF-8 input as an input error, and writes through symlinks.
     - Its tests grew from 12 to 21. On the pilot copy it still adds 536 links on 343 statements and tags 509, and a second run changes nothing.
   - **Layout.**
     - The ⓘ footer again fills its frame.
     - Nested Markdown in a statement is not re-centred.
     - The status button has one width, so statement text has one width.
     - The controls wrap below the text in narrow panes.
     - Compare's Before and Deleted rows keep the text measure, and 18px text applies only to current statements.
   - **Performance.** Opening a fold or a panel re-rendered and re-parsed the whole document (380–760 ms per click in a development build). Fold and panel choices now live in a store each statement subscribes to, so a click re-renders one statement.
   - **Records.** The guide names the fields the viewer reads, and the reader contract matches the code. The design-verified examples link a state machine. The ontology statements state the conflict rule, the Checking… state, roll-up without inheritance, alias refusal and the layout's relation to the text measure.
   - **Verification.**
     - The full frontend unit suite passes (366 files, 5,627 tests). Rust parser, exclude and Diff Graph tests and clippy pass.
     - The browser suite passes apart from the 6 chat and centre width tests that fail on `main`. A new browser check keeps Compare's Before and Deleted rows at the measure.
     - On the pilot repository's Receiving page in a real browser, statement text has one width and fold and panel clicks take 60–100 ms in a development build.
     - Re-checked on the local full-stack preview with `.g5/TLA_verification` hidden from Files, in a stopped workspace that reads from Git. 28 statements show Design verified and 1 Unverified, from 16 direct reads and no listings. The panel shows the runs, and a metadata `#L` link opens the knowledge article.
   - **Unrelated dev-tooling bug:** a preview restart hung in `scripts/stage-local-worker-runtime.sh` line 780. The plain `mv` stopped at an overwrite prompt for an existing root-owned `claude-code.version`. Left unchanged.
10. **Owner feedback on the preview (2026-11-01)** ([decisions 15–17](../../chat_intent/ontology-formal-verification/verification-display.md#owner-feedback-on-the-preview-2026-11-01)).
   - **All Unverified outside Receiving.** This was my seed, not the viewer. It held only the 15 state machines linked from the Receiving page, out of the pilot repository's 180, so every other page found no manifests. The preview was reseeded with all models, the run index and the receipts.
   - **Dot.** A statement with nothing to open now has a drawn 9px dot, just under the triangle; a child's is 7px. It replaces a 22px `•` glyph that rendered much smaller than the drawn triangle.
   - **Unverified and Checking…** are plain labels and open nothing.
   - **Other tags.** Test Coverage, Manual checking and review and Implementation verification show the statement's metadata details; linked state machines show only for a design-verified tag. Retry remains for a Design verified statement's unreadable state machine.
   - **Rich text, not changed here.** A statement whose lead sits directly under its marker opens in Rich text as one protected "Record line" showing raw source. `markdownDocument.ts` protects the whole paragraph that starts with identity lines; it is unchanged by this work, and the platform shows the same. Files cannot be edited on this VM's preview because workspaces cannot start. Splitting the lead from the identity lines in Rich text, as Preview now does, is a separate change.
   - **Tests.** Unit and browser tests cover the no-details Unverified label, metadata details for other tags, Retry inside a Design verified panel, and `metadataDetails`.
   - **Double dot, fixed.** The first dot rule also matched the statement row, which carries the marker's class, so top-level statements without details showed two dots. The rule now requires the marker, and a browser check measures the 9px dot and asserts the row draws none.
   - **Preview data aligned with the guides before seeding.** The pilot data was aligned with the guides in a copy before seeding; nothing from it enters a G5 repository.
     - **Audit.** The artifacts already matched the artifact guide: 179 manifests with questions and labelled runs, and 7,205 index rows with concern, label, outcome, timestamped run IDs and receipts. The only exceptions were a runner self-test folder with no manifest, and 72 rows for labels no longer declared, which are shown as retired runs.
     - **Normalization.** It added the guide's blank line after each of the 608 markers, made 614 tag bullets bare (moving 25 notes to their own bullets), and encoded 100 link destinations with spaces. It also added alias anchors for the two moved IDs that five manifests still named. `check_ontology.py` went from 102 issues to 0.
     - **Helper.** It then added 540 links and 510 tags, with no notes left.
     - **Offline prediction.** The viewer's own functions predicted 515 Design verified and 93 Unverified statements, plus 44 Human and 91 Uncertain badges; the original showed none of those badges.
     - **Real UI.** All 22 pages in the real preview matched that prediction.
10. **Findings triage (2026-11-02).** See [Findings triage](#findings-triage).
    - **Agents:** `verification-review.md` and `check_findings.py` (17 tests), sharing two fixtures with the viewer's parser. `tla-verification.md` is `main`'s again. G5's own `.gitignore` tracks only the two findings files in `.g5/TLA_verification/`.
    - **Viewer:** `lib/findings.ts` (format, decision rewrite, issue body, metadata links), `lib/findingIssues.ts` (shared issue routes), `findingsMarkup.ts` and `FindingsControls.tsx` (control rows, filter, decisions), the Core Ontology entries, the Design verified panel section, and FileView's decision write. `CreateIssueDialog` gained a fixed creation key and a title-and-description-only mode.
    - **Performance:** a findings file renders without the 128 KB preview deferral. Its rendered document is memoized, since ReactMarkdown parses on every render and a decision re-renders the file view several times: in a development build, the pilot repository's 717 KB file took 16.5 s per decision before and about 3-5 s after. The filter is one attribute on the document in its own component (39 ms to hide, 195 ms to show, measured in the page).
    - **Tests:** unit and component tests for every decision path, the filter, anchors, read-only reasons, refused and stale writes; browser specs `findings.spec.ts` and the verification spec's findings section; the full-stack flow `findings-triage.spec.ts` (CI only, against the native tracker).
    - **Pilot dry run:** a separate agent followed the guide on a copy: 440 bug entries (218 actionable) and 91 simplifications (72 actionable), 250 old links remapped, severities assigned, then 398 back-links added; all checkers clean. Its notes produced the severity scale, the root exemption, the `Findings:` line and the snapshot comparison.
    - **Real UI:** on the local preview, rebuilt from this branch, the seeded pilot workspace lists both files in Core Ontology, renders all 440 entries with severity counts, and the Receiving page's Design verified panel lists its linked findings and opens them. Decisions could not run there, because the preview starts no workspace VMs; the flow test covers them in CI.
    - **Review fixes:** two independent reviews. Retry, Check again and Confirm now act on the current entry, file and gate; a refused write drops its draft and re-reads the file after a version conflict; Irrelevant finds and closes an issue an earlier press filed; an untrusted retitle sends only the title reviewed; review dialogs offer no assignee or parent; links to an entry's anchor reach it; `--previous` compares with a pre-edit copy because decisions may be uncommitted; the checker reads links, anchors and sub-headings as the viewer does.
11. **Option 2, regression review and the pilot end to end (2026-11-02).**
    - **Option 2** (decision 23): the guide's actionable entries now include findings that need a product or design decision first. On the pilot repository, 172 more bug entries became actionable: 390 of 440, against 218 before.
    - **PR-wide regression review** (independent, against `main`): nothing breaks for ordinary documents. Three changes reached past the stated scope; two were fixed:
      - Findings files are only `.g5/TLA_verification/` or `TLA_verification/` in a `.g5`-rooted checkout, no longer any `TLA_verification` folder, such as one in Project Docs.
      - Core Ontology looks for them only in repositories with an ontology, every five minutes, keyed by the read authority.
      - Kept as the links rule intends: ontology links in All changes also try the fallbacks and say when no file exists. Neither ontology here has folder links, the case a snapshot read cannot confirm.

      The memoized findings document now follows its callback props, and answers write through the host's current rewrite.
    - **Browser suite:** 84 passed, 6 failed. The 6 are `markdown-layout` width tests, which fail identically on `origin/main`.
    - **Full unit suite:** 376 files and 5,708 tests pass.
    - **The pilot end to end on the local preview,** with a workspace VM running once a slot was freed:
      - one entry went Need fix, Defer, Irrelevant, then Need fix, on one native issue that was retitled, closed and reopened;
      - an uncertain-intent entry's Defer filed a `[Low]` issue;
      - Irrelevant only marked the file;
      - a simplification's Need fix filed an issue;
      - the decisions stayed uncommitted, and the Receiving panel showed the new decision;
      - decisions survived a VM stop and restart;
      - a press interrupted mid-write was recovered by the creation key, without a duplicate;
      - the existing display passed on the same workspace: 28 Design verified, folds, panel runs, `#L` links, and an ordinary Markdown file without controls;
      - opening the 440-entry file took about 6 s in the production build.
    - **Rebuild after a VM restart:** the owner's personal repository holds the seed rebuild script, the recipe, the agent prompt and these checks. The rebuilt seed was checked against the earlier one.

