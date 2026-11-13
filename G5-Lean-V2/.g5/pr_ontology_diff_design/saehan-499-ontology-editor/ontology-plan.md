# Core Ontology: current-format discovery and editing

The discovery/editor work and subsequent refinements have implementation evidence below.
The core statement comparison is implemented, with scoped evidence below.
The all-changes overview is implemented in the working tree on 2026-10-16;
its verification and deployment limits are recorded in the amendment below.
Engineering choices are agent decisions within the recorded human scope. Historical
verification sections describe their named revisions, not proof of this new feature.

## File-backed integration — 2026-10-19

The human [selected Option 1](../../chat_intent/saehan-499-ontology-editor/file-backed-integration.md).
This amendment supersedes node-based integration choices and their historical evidence below.
Keep the sidebar, rich editor, Compare, All Changes and `.g5/ontology` in Files.
Reconcile [node retirement](../../ontology/features/documents.g5.md#g5.documents.node-retirement)
with [file-backed navigation](../../ontology/features/documents.g5.md#g5.documents.ontology.navigation).

Shared Files discovery owns authorized live/committed source selection and bounded path enumeration;
workspace-file access owns content, hash/VM preconditions and explicit selected-file publication.
The UI identifies tabs, comments, references and caches by workspace/repository/path. Statement IDs
remain authored anchors within files. Ontology parsing and presentation stay in the shared editor.
All Changes retains bounded server-side comparison with captured parent/source revisions, complete
inventory checks and visible failure; replace only its node inventory and navigation identities.
Do not restore node APIs, database projections, workers, legacy navigation or request-time indexing.
An absent authorized `.g5/ontology` discovery root is an empty listing in both live and committed
Files reads; missing repositories, permission failures and other missing paths retain their errors.
Single-file comparison captures the readable parent/repository revision authority using Source
Control, closes its scoped transaction, then reads bounded historical content, like the overview.

Use a file-backed rich tab for Core Ontology with repository-qualified identity; Files remains the
source repair surface. Preserve draft isolation and navigation safety when switching views. Direct
file reads do not depend on successfully enumerating every repository. Discovery errors cannot mean
empty; compare errors cannot erase drafts or imply zero changes. Keep main's newer discussion/review
and file-authority behavior. Port shared `.g5/ontology` path classification, including nested runtime
and Git exclusions, without weakening the general Files policy.

Required checks cover parser/editor and rich review regressions, live/committed multi-repo discovery,
read/write authorization, draft and stale Save/VM behavior, references/comments, Compare/All Changes,
retired-route/storage absence and real-stack browser editing/comparison flows. No earlier results
establish this integration.

Implementation reconciled onto main `404f8b3570af4013c3b64d9e76471d2815d08ac1`, preserving the
four feature commits and one consolidated repair commit. The original published tree is retained
at `backup/ontology-editor-before-squash-rebase-20261019`; the pre-rebase five-commit history is
retained at `backup/ontology-editor-squashed-before-file-rebase-20261019`.

The sidebar now reads the shared Files subtree and opens repository/path file tabs with an ontology
presentation. Core and Files share drafts, hash/VM preconditions, comments and cache invalidations;
reloadable URLs preserve the chosen presentation. The comparison overview opens the same file
identity. Tests cover refreshed children, repository disambiguation, incomplete/error listings,
draft handoff, workspace Discard, failed saves, replacement drafts and edits during a pending Save.
Historical sidebar defaults migrate explicitly; custom section placement remains intact.

Fresh verification on 2026-10-19:

- Full frontend suite: 298 files / 4,675 tests passed. Production frontend build passed;
  ESLint reported zero errors and 506 warnings.
- Exact CI command `cargo clippy --workspace --all-targets -- -D warnings` passed in the local
  Linux `g5/base-rust-warm:26.04` image. The native macOS invocation encounters dead-code diagnostics
  in unchanged main-branch Linux runtime modules; no allowances were added to suppress them.
- Backend: nine ontology API tests, four current-reader transaction-boundary guards, three
  projection-retirement tests, eleven route-authorization audits, 224 semantic tests and nine
  shared path-policy tests passed. Rust formatting and whitespace checks passed.
- Seed helper unit suite: nine passed, nine opt-in live cases skipped; shell syntax passed.
  The complete shell seed also passed against a fresh `pr606-verified` workspace, including
  live readiness and shared Files discovery/content verification.
- Real-stack stopped-workspace Chromium coverage passed for Core, rich reading, reloadable
  file/overview routes, All Changes and `.g5/ontology` in Files. The bare-record shared seed
  correctly reports missing authored anchors in Compare instead of claiming zero changes.
- All seven affected Chromium scenarios passed across `content-loading`, `current-ontology`
  and `ontology-comparison` (plus authentication setup): live rich editing, draft handoff,
  persisted Save, authored identity/footer preservation, comments, reference navigation,
  creation/rename refresh, missing-target errors, Compare/Before/Undo, All Changes reopening
  and a competing file-write conflict. One new overview assertion initially targeted a region
  role that its labeled file container does not have; the corrected selector passed on rerun.
- The human-approved temporary process-worker admission on this Mac was restored to `false`,
  verified in the running API container. The temporary host was stopped. These are real API,
  Git and browser checks in process mode; they do not establish Linux/KVM runtime behavior.

## Outcome and governing contracts

Core Ontology should discover `.g5/ontology` and open its current Markdown format,
as requested in the [human source](../../chat_intent/saehan-499-ontology-editor/core-ontology.md).
The proposed ontology addition is [current-format support](../../ontology/features/documents.g5.md#g5.documents.ontology.current-format).

Preserve [multi-repository ontology navigation](../../ontology/features/documents.g5.md#keep-document-and-ontology-product-value),
[block editing](../../ontology/features/documents.g5.md#retain-block-style-spec-editing),
[source editing](../../ontology/features/documents.g5.md#retain-source-editing),
[inspectable relationships](../../ontology/features/documents.g5.md#make-intent-traceability-inspectable),
[selected-source truth](../../ontology/features/documents.g5.md#show-the-selected-durable-source-truthfully),
[bounded reads](../../ontology/features/documents.g5.md#bound-every-readers-tree-and-file),
[Save preconditions](../../ontology/features/documents.g5.md#save-binds-bytes-and-the-attachment-they-came-from)
and [file comment identity](../../ontology/features/documents.g5.md#a-files-comments-follow-the-file).
The [authoring format](../../../.agents/skills/ontology-authoring/references/format.md)
defines the target syntax, including index-only pages and per-block footer metadata.

## Pre-implementation evidence and implications (historical, before file-backed integration)

- `ontology_service.rs::get_ontology_tree` lists only `.semlift/specs` and filters
  `.semlift.md`; its parser requires YAML `component_id` matching that path.
  The inspected HEAD has 0 matching files and 34 Markdown files in `.g5/ontology`.
- The current corpus contains `.g5.md` and ordinary `.md` pages. It uses headings,
  `[I · stable.id]`, `[D · stable.id]`, `[Q · stable.id]`, normal Markdown links,
  and a final `## Metadata` section. Index pages need no commitment blocks.
- `WorkspacePage.tsx` routes ontology documents into `SpecEditor`. The existing
  record parser in `frontend/src/lib/utils.ts` expects legacy numbered labels;
  the backend markup/resolution types also carry legacy label metadata and refs.
  A successful directory listing alone does not supply semantic editing support.
- Explain-in-chat, composer context, semantic reads, synchronization, grounding,
  comments and restored tabs consume node identity or legacy component paths.
  They must be checked before describing the migration as complete.

## Settled product decisions

The [human answers](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-decisions--2026-10-13)
settle all five questions. Their governing behavior lives in the documents ontology:

| Decision | Selected behavior and owner |
|---|---|
| Sidebar hierarchy | [Repository/folder/file navigation](../../ontology/features/documents.g5.md#g5.documents.ontology.navigation): omit nonqualifying repositories, flatten exactly one, group several. |
| Blocks and metadata | [Current-format presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation): rich content editing, read-only kind and collapsed Metadata containing ID and footer details; no statement-title display or controls, ID renaming or human statement movement. |
| Statement creation | [Ontology insertion](../../ontology/features/documents.g5.md#g5.documents.ontology.creation): Add Statement creates Intent; Heading creates H2, without submenus. Other document menus remain unchanged. |
| Legacy format | [Manual migration](../../ontology/features/documents.g5.md#g5.documents.ontology.migration): no legacy discovery, fallback, compatibility aliases or automatic conversion. |
| Relationships | [Ordinary links](../../ontology/features/documents.g5.md#g5.documents.ontology.links), [block previews](../../ontology/features/documents.g5.md#g5.documents.ontology.previews), [visualization deferral](../../ontology/features/documents.g5.md#g5.documents.ontology.graphs) and [shared-editor trace removal](../../ontology/features/documents.g5.md#g5.documents.ontology.tracing) define the scope; the retired combined resolved-context panel stays out. |
| Moves | [Authored link maintenance](../../ontology/features/documents.g5.md#g5.documents.ontology.moves); stale links/tabs report a missing target. No automatic redirects, tab relocation or comment transfer. |

The [human correction](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-correction--2026-10-13)
excludes legacy graph compatibility and defers dependency visualization. Current-
format reference navigation remains in scope. The later shared-editor amendment removes editor trace controls. Ordinary
links must not be synthesized into dependency or trace edges.

## Engineering decisions

- Discover all Markdown files recursively inside exactly `.g5/ontology`, including
  index pages; exclude sibling plans/reviews and do not require YAML or a root file
  to make other files visible. Parsing diagnostics do not erase files from the tree.
- Keep repository content authoritative. Reuse actor-aware Files listing and content
  selection: live checkout for the attached running owner, committed content for
  other readers/stopped work, with the existing labeled fallback policy. The captured
  `RepoReadView` is committed-only; it cannot discover an owner's uncommitted files.
  Extract/reuse the Files routes' authorized helpers rather than adding a second
  source-selection policy, independent index authority or file-watching lifecycle.
- Keep discovery/identity in shared file readers and repository-qualified UI state, format facts in a pure shared
  parser, and presentation/local draft state in the editor. Reuse confined file
  read/write services and existing refresh events. Do not fabricate SemLift paths,
  component IDs or compatibility aliases for current-format documents.
- Count a repository as containing ontology when successful discovery finds at least
  one Markdown file under `.g5/ontology`, including an index-only or malformed file.
  Empty directories do not qualify. Keep the existing read/error distinction for a
  failed repository discovery rather than silently classifying it as empty. Put
  `root.g5.md` first. Use file-stem labels in the sidebar and document titles in the
  opened document, avoiding whole-corpus reads just to label the tree. Keep block
  outlines in the opened document rather than making blocks sidebar children.
- File identity remains workspace/repository/path. Stable IDs identify authored
  blocks within their document; retain repository context and diagnose duplicate
  targets without choosing arbitrarily. Resolve links by their authored path and
  fragment; do not search elsewhere for a moved block to rescue a stale path.
- Adapt the existing reference popover for links whose fragment resolves to an
  ontology block. Heading association comes from the target document's parsed source;
  do not equate ordinary heading slugs with stable block IDs. Whole-document links
  navigate directly, and non-block headings retain normal heading navigation.
  Footer links use the same authorized navigation without changing link meaning.
- Preserve ordinary relative-link repository context even when a link exits
  `.g5/ontology`; it opens a file/document rather than extending ontology membership.
  An explicitly repository-qualified link must resolve that repository under the
  existing workspace authorization. Missing and ambiguous targets remain explicit.
- Preserve Markdown bytes outside an intentional edit. A malformed block, duplicate
  ID or missing metadata produces a visible diagnostic and recovery through agent
  edits or the existing Files source editor;
  unsafe structural actions on that span are unavailable. A Q block is an unresolved
  ontology question, not automatically an executable question-set entry or answer.
- Never turn loading, authorization, parser, size-limit or refresh failures into
  “No ontology nodes.” That message applies only to a successful empty discovery.
- Listing uses the existing Files limits (10,000 entries / 3 MiB entry data per
  repository); carry `truncated`, source and live-fallback information through the
  ontology response. Process member listings sequentially and apply the same entry/
  byte ceilings to the combined response. If the workspace ceiling is reached, report
  incomplete discovery rather than deciding the omitted repositories are empty.
  Flattening one repository requires complete discovery; otherwise preserve repository
  context and an incomplete/error notice. Listing walks recursively within those
  bounds, not a silent arbitrary depth cutoff.
- The sidebar reads no file bodies. Opening or previewing reads/parses only one target
  file, at most the existing 32 MiB file limit, with no transitive expansion or batch
  corpus parse. Same-document previews reuse local editor bytes; cross-document previews
  use the actor-aware read and its source/version. Oversize or invalid text remains a
  visible file with an explicit read/parse diagnostic; never an empty-tree result.
  This removes the unbounded aggregate parse instead of adding an indexing lifecycle.

## Implementation readiness verification — 2026-10-13

Historical node-based wiring below is superseded by the file-backed integration amendment; source-selection and bounded-read obligations remain.

Source inspection found and resolved these gaps in the earlier draft:

| Inspected boundary | Required integration |
|---|---|
| `workspace_reader.rs::resolve_connected_binding_view` constructs committed-only views; `routes/files.rs::list_file_tree` already selects live owner or committed reader content. | Reuse/extract the Files listing helper for ontology discovery. Preserve per-repository source/fallback labels. Ordinary file reads and Save remain owned by the existing workspace file helpers. |
| `routes/ontology.rs::node_content_path_candidates` and projected-node lookup assume legacy component paths and committed membership. | Derive node UUIDs from workspace/repository/path plus node kind; flattening never changes them. Carry real file paths and resolve file nodes through actor-aware discovery, so live-only files are openable/saveable. Directory clicks only expand; no graph request. No durable node registry or legacy fallback is needed for current navigation. |
| `SpecEditor`'s `RefExpansionPanel` / `useReadOntology` expect `component#block`; Markdown rendering already has heading-slug support. | Reuse the popover presentation, replacing its current-format resolver with one authorized target-file read and the current parser. Carry repository/path/fragment in navigation, including Back and restored tabs. Reuse the same heading-anchor algorithm for rendering and resolution, including repeated headings and explicit authored anchors. Do not restore the combined resolver panel. |
| `ws.ts::refreshLiveContent` refreshes node bodies but omits ontology roots. | Add ontology discovery/reference invalidation to the existing coalesced file-change handling, for every member repository. Preserve durable-cut/authority invalidation and local dirty buffers. No job, commit or legacy index event is needed to reveal a live new file. |
| Grounding routes return `results: []` even when no record identity can be resolved. Their natural key already accepts a string identifier. | Look up explicit records by repository, current file path and exact stable block ID; adapt extraction where needed, never translate to numbered IDs. Expose unavailable/not-recorded versus supported-empty versus results/error. A matching explicit record with zero edges can truthfully have no relationships; missing or unreadable records cannot. No migration or creation of grounding records from Markdown is authorized. |

The [statement-editing correction](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-correction--2026-10-14)
limits human editing to the supported rich-editor actions. Statement titles are not
shown or editable in the statement UI; kind is visible and ID is read-only inside collapsed Metadata. Preserve authored
headings in Markdown and their navigation anchors without rendering them as statement
titles. Document titles and ordinary grouping headings are unaffected. There is no
human statement move/reorder action or ID-renaming mechanism in this change. Source
access does not add an editable raw-source mode inside Core Ontology; existing Files
source editing and agent file editing remain available for repairs. This restriction
belongs to the Core Ontology editor, not a new server-side ban on ordinary file edits.
Keep statement-title text out of generated outline labels, tooltips, popover headers
and deletion dialogs as well as statement cards; use kind there and disclose IDs only under Metadata. Authored link
text, document titles and ordinary grouping headings remain unchanged. Preserve hidden
statement-heading anchors and their document-order slug numbering so links still land
on the visible statement, including when titles are duplicated.

Recognize heading-associated I/D/Q markers outside code fences, and pair footer entries
by exact stable ID, not position. Human content edits preserve IDs, authored headings,
and unrelated source spans. Supported deletion removes the statement and only its
unambiguously paired metadata in the same draft/Save. Orphan/duplicate metadata stays
visible with diagnostics rather than being discarded. Nested formal statements retain
their own identity and metadata. Offer structured deletion only when the statement's
heading/marker/body source spans and one footer entry are uniquely identified and
contain no other statement. Otherwise explain why deletion is unavailable and leave
repair to agents/Files; do not cascade-delete, promote nested statements, or guess.
A supported deletion removes those statement spans and the paired footer entry as one
local edit (and one undo), with ordinary explicit Save and failed-Save preservation.
Revalidate against the current draft when confirming deletion. Keep unrelated links
and explicit grounding records untouched; stale targets use existing diagnostics.
Provenance remains as authored, never promoted to verification, approval or permission.

Agents maintain IDs, metadata and affected Markdown links when moving ontology under
the existing authoring rules. The UI rereads the resulting files through existing
refreshes; it does not track moves or infer ID renames. An inconsistent external edit
produces diagnostics rather than automatic footer-key repair. New authored statements
must have valid, locally unique stable IDs, without introducing an ID-edit control or
legacy numbered-label generation.

Resolve relative links from the source document's directory within its repository;
permit `..` inside the repository, reject escapes, and preserve heading fragments.
Use the existing heading slug/explicit-anchor semantics, not guessed stable-ID aliases.
A link to a block heading opens its preview; a non-block heading opens and scrolls;
a missing file or fragment reports the missing target instead of silently selecting
the document's first block. External URLs retain ordinary safe link behavior. Source
line anchors use existing source navigation; they are not ontology block references.

Current-format semantic consumers receive the new parser facts and real file identity.
Shared Project Docs/question-set behavior remains governed by its own contracts; dropping
legacy ontology compatibility does not authorize breaking those consumers. Legacy-only
node sync/graph affordances are not shown for current file nodes; ordinary agent workflows
receive the real path and applicable authoring skill. Tests must cover these unchanged
consumers where shared code changes.

## Revision verification — 2026-10-14

The current `SemanticRecordFrame` always mounts `RecordTitleEditor`; `SpecEditor`'s
outline exposes heading text in labels/tooltips, and the old deletion helper removes
one contiguous record span without pairing a footer. Reuse requires current-format
presentation/action boundaries, not CSS-only title hiding. Rich body editing must not
include protected headings or ID markers through selection, paste or keyboard commands.
Project Docs retain their independently governed controls.

The [human creation decision](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#human-creation-decision--2026-10-14)
settles the remaining question: leave new statements to agents/Files and remove Add
I/D/Q from Core Ontology. The existing `openRecordComposer` / `handleSaveRecordComposer`
requires a title, generates numbered IDs and writes legacy inline metadata; do not
expose it through current-format ontology buttons, menus or keyboard entry points.
Preserve ordinary body-content editing and supported deletion. Shared Project Docs
controls remain governed separately. No replacement creation or ID-generation flow
is part of this change.

## Implementation discovery — shared path classification

The real API integration test found that `g5_paths::is_files_tree_visible_path`
classifies `.g5/ontology/**` as runtime metadata. Merely selecting the new root
therefore still returns an empty tree, and ordinary Files writes are blocked by
the same classifier. Treat `.g5/ontology` and its authored contents as durable
repository content in the centralized policy and canonical Git exclusion rules.
Keep nested `.g5` sidecars and `.git` metadata excluded. This is required by the
approved Files repair/creation and current-format discovery contracts; it does
not make sibling `.g5` plans or runtime files ontology members.

The real browser run also found that the live Files tree reports a missing root as
`FILE_MISSING`. Ontology discovery treats exactly that result for its requested root
as a complete, empty live listing. Other runtime/read failures remain visible and
ordinary Files missing-path behavior is unchanged. This preserves the selected rule
that repositories without ontology do not count against one-repository flattening.

Changed-path categorization recognizes current ontology Markdown as ontology content,
without enabling legacy synchronization or inferring dependency edges. Existing
node comment threads remain accessible from restored folder tabs, independently
of the retired graph view. Current document nodes are derived navigation IDs, not
persisted legacy node rows: their comments use repository/file identity only. The
browser run exposed an unnecessary legacy-node comment query returning 404 beside
successfully saved file comments; suppress that query for current-format documents.

Semantic reads use the same bounded actor-aware file content helper and return
one file's current parser facts with its real path and source/version. The old
`read-ontology` corpus resolver and component-reference expansion are retired
from that route; current requests for transitive/dependency expansion report
that the operation is unavailable. No combined-context panel is added.

## Implementation sequence

1. Implement the verified reader, identity and bounded-listing integration above;
   remove legacy discovery assumptions without adding compatibility. Keep authority
   rechecks in the file helpers: discovery is navigation, not a grant to read or Save.
2. Add current-format discovery and scoped identity, then connect content reads and
   refreshes. Adapt the shared parser and editor with parity fixtures based on the
   real corpus, preserving index prose, fences, diagrams, tables and footer metadata.
   Remove Core Ontology Add I/D/Q entry points while preserving body editing and
   supported deletion; keep independently governed document controls intact.
3. Integrate Save, comments, relative-link/heading navigation, block popovers,
   clickable footer metadata, composer context, Explain in chat and tab restoration.
   Retain explicit-record tracing with a distinct unsupported state. Exclude legacy
   graph compatibility and defer dependency visualization; keep the retired
   resolved-context panel out. Adapt semantic consumers so legacy-only actions cannot operate
   on current files under fabricated paths or silently ignore current records.
4. Verify against contracts and real interfaces, fix discrepancies, and update
   material ontology evidence. Implementation is a separate authorization step.

## Acceptance and verification

Real API/browser coverage must establish discovery → open → rich content edit →
Save → reload, with ordinary links, comments and agent context naming the correct
file. Cover zero, one and several qualifying repositories, mixed qualifying and
nonqualifying repositories, equal paths in different repositories, and malformed
files that remain discoverable. Check live-owner files versus durable reader/stopped-
work views; create/edit/delete/rename refresh; restored tabs; and absence of legacy
files from the current ontology tree. Retain hash/VM conflict refusal and local edits
after failed Save or refresh. The inspected 34-file corpus should all be discoverable,
including its plain `.md` index; test exact contents, not just a count.

Parser/editor fixtures cover heading-to-ID association, every I/D/Q kind, metadata
pairing and provenance distinctions, formal expansions, stable-ID preservation, index-only
pages, code fences containing marker-like text, unknown Markdown/metadata, duplicates,
malformed labels and missing footers. Assert untouched-byte preservation and frontend/
backend agreement. Exercise broken links, unauthorized targets, excessive depth/width,
large individual files and aggregate discovery limits without a false empty tree.
Verify the sidebar does not read document bodies, and popovers read only their target.
Cover live-only new files without an active job, watcher-driven refresh, partial member
failures, incomplete listings, source labels, and unchanged dirty editor contents.
Verify Core Ontology has no Add I/D/Q button, menu command or keyboard composer
entry point, while agent/Files-authored statements appear after refresh and can be
edited or safely deleted. Verify shared document creation controls are unaffected.
Check absence of generated statement-title display/edit controls in cards, outlines,
tooltips and popovers, and absence of ID-edit and statement move/reorder controls.
Check heading-anchor navigation after title hiding, preservation of authored link text,
and that generic body-edit/paste/keyboard paths cannot modify protected source. Verify content edits preserve IDs and authored headings, supported
deletion removes its heading/marker/body and only its paired metadata atomically;
undo restores both, failed Save preserves the draft, and confirmation revalidates it.
Verify ambiguous/missing metadata and statements containing other statements refuse
structured deletion without mutation, while unrelated statements/metadata survive. Agent-authored moves
refresh correctly.
Check diagnostics for agent-authored ID/metadata mismatches, duplicate heading slugs, same-file
previews of unsaved bytes, stale fragments, and shared Project Docs/question behavior.
Exercise relative links to documents/source files and heading fragments in the
correct repository; block previews and “Open document”; direct whole-document
navigation; and clickable footer links preserving their provenance/evidence meaning.
Verify the shared editor and read-only frames have no Source/Drives controls or trace
requests, while the independent intent-log trace consumer remains intact. Verify IDs
are hidden until Metadata opens, including missing-footer records, and current-format
navigation requires no legacy graph compatibility, no ordinary link creates a dependency/trace edge, and neither dependency
visualization nor the retired panel is introduced. After an agent-authored move,
updated links work and stale links/tabs clearly fail without automatic relocation,
redirect creation or comment transfer. Unit coverage supplements end-to-end evidence;
the executed implementation evidence below is scoped separately. No TLA+/TLC is requested.

## Design self-review

All five human choices are recorded and consistent with the owning ontology. The
selected design derives navigation and reuses content authority rather than adding
durable move tracking or a second metadata owner. Legacy format compatibility is
out of scope, including legacy graphs; dependency visualization is deferred.
Rich content editing, existing Files source editing, ordinary Save and comment identity
are preserved. Statement-title controls, human statement movement and ID renaming are
excluded; authored headings and IDs remain intact through content edits. Links remain
links, Q blocks remain ontology questions, and unsupported tracing is not an empty
relationship claim. No combined resolved-context panel is reintroduced.

The five original product decisions and the statement-editing correction are
consistent with the accepted agents/Files creation decision. No product question
remains open for this scope. Engineering boundaries have concrete owners and
behavior above, including live-source discovery, resource bounds, node identity,
reference resolution, refresh, tracing availability and safe deletion. These
are the implementation contracts. Amend this same plan before dependent work if a relied-on guarantee proves insufficient. Executed verification does not imply human acceptance, publication, TLA+/TLC or a separate deep ontology review.


## Implementation evidence — 2026-10-14

The working tree implements actor-aware, bounded `.g5/ontology` discovery, current-format
parser facts, repository-scoped navigation and previews, protected body editing with
paired deletion/undo, authored footer disclosures, explicit-record tracing, and normal
file refresh. Core Add I/D/Q and current dependency-graph/resolved-context entry points
are removed. Existing Project Docs and file/node comment identities remain supported.
Changed-path classification recognizes current ontology without enabling legacy sync.
A stale same-named path in another repository cannot rescue a missing target.

Executed checks:

- Contracts library: 391 tests passed; semantic Markdown parser: 116 passed; ontology
  route unit tests: 5 passed. The final shared parser fixture also passes after adding
  malformed-marker and orphan-footer diagnostics.
- Real HTTP ontology integration: 29 tests passed; current semantic-read API: 3 passed.
  The focused current-format journey passed again after adding aggregate discovery
  bounds (12,000 entries across two live repositories) and asserting no body reads.
  It passed again after the real browser exposed the missing-root case; absent live roots are complete empty members. It covers multiple repositories, live-only files, Save/hash conflicts, explicit
  grounding, malformed ledgers, and absent versus recorded-empty relationships.
- Existing Files bounds/live-tree integration: 8 tests passed; all 6 live-tree tests passed again after the missing-root change. Shared editor/utils
  regression: 652 passed. The final focused parser/editor/sidebar/navigation/utils
  run passes 298 tests. Directory comments were restored after regression detection;
  all 63 WorkspacePage comment tests pass, including suppression of legacy-node queries for current files. The final comments/navigation/editor run passes 78 tests. Other navigation/hook checks passed.
- Frontend production build and TypeScript checks pass. Targeted lint has no errors
  (one existing-style OntologyTree effect warning). `git diff --check` passes.
- A disposable Git repository confirmed the canonical exclusion patterns retain
  `.g5/ontology` while excluding nested runtime sidecars. These constants are not
  automatically installed into existing checkouts; no exclude-file rewrite is claimed.

The real-browser journey in `frontend/e2e/flows/current-ontology.spec.ts` passes
with Chromium against the rebuilt API and a real native process-mode workspace
(2 tests including authentication, 30.1 seconds). It uses actual HTTP and file
operations without route interception. It verifies one-repository flattening,
full IDs and hidden statement headings, rich body editing and persisted Save/reload,
comment file identity without a false legacy-node error, Explain-in-chat path context,
local and cross-document previews, watcher-driven discovery, file rename, stale tabs
and links, corrected authored links, and paired deletion/undo. The final frontend
production build also passes after the comment-query correction.

The strict Docker lane passed preflight and built a candidate, but stopped at phase
4b because this Docker host has no `/dev/kvm`. Source changed while that initial
candidate was building, so its receipt does not validate the final source. The
separate supported localhost process-mode run above validates the feature flow;
it cannot establish microVM parity. Rerun `just ci-docker-7
e2e/flows/current-ontology.spec.ts` on a KVM-capable host for that remaining gap.
The preflight's obsolete literal cargo-command assertion was updated to match the
already-existing package-array build; the strict microVM harness is unchanged.

Document checks confirm 101 uniquely paired governing IDs/footer entries and valid
local plan/human-source link targets; `git diff --check` passes. The temporary native
worker/frontend and isolated test containers are stopped after verification. Existing
development and unrelated stacks are preserved.

Implementation and local process-mode verification are complete. Strict microVM
validation remains unavailable here. No commit, push, deployment, or human acceptance
has occurred.

## Lean self-seeding follow-up — 2026-10-14

Authorized source: [human request](../../chat_intent/saehan-499-ontology-editor/lean-seed.md).
Governing: [local seed contracts](../../ontology/features/operations.g5.md#g5.operations.lean-seed).

Add `just seed-lean-workspace`: pin local HEAD once, fetch its reachable history
into an independent staging repository (no alternates, source remotes, local hooks
or shared objects), and initialize an empty docs commit. Preserve the source tree
exactly, including `.g5/ontology`; uncommitted files never enter staging. Reject
shallow sources and submodules until their independent-history semantics are supported.
Default name G5-Lean, organization g5labs, local dev owner demo@g5labs.ai. Resolve
stack identity through the existing helper; require loopback API and test auth.

The generic seeder cannot be reused as-is: it resets targets, requires SemLift and
recreates snapshot commits. Use dedicated repos with exactly one ordinary standalone workspace (`is_base=false`,
`cloned_from_id=NULL`). Install new UUID-scoped directories before a single database
transaction publishes the target, settings and members. Do not create seed-source
workspaces or refs. Normal forks of this root retain parent comparison, rebase and
merge behavior; the root itself has no upstream workspace. Brief exclusive table locks serialize the final
name and pending-fork check, including in-flight name readers that could otherwise
insert after the seed commits. Lock acquisition is bounded to 15 seconds; all Git
work happens before this short transaction. Git and DB are not one atomic
store: failed staging can leave unreferenced UUID directories, which are reported;
never automatically delete directories after an uncertain transaction result.

An existing name fails before staging by default. `--replace UUID` only accepts the
exact owner-held workspace marked by this seeder, prepares the new source first, and
uses the ordinary non-forced delete API. Do not bypass busy/stop failures. Deletion
and new creation are deliberately separate; report a failure after deletion and
permit a normal rerun if no active target remains. Retain old repos according
to the product deletion policy. No stack restart, implicit VM resume or changes to
other seed commands. The user may start compute normally after creation.

Verification: exercise actual Git staging with unpushed ancestors, dirty/untracked
exclusion, source movement after pinning, empty docs, shallow/submodule refusals;
real local seed, duplicate refusal, replacement wrong-ID refusal, imported exact
SHA/tree and ontology paths/content, unchanged existing workspace/source state, and
browser navigation. Test allowed replacement on a disposable seed workspace via
the actual lifecycle route. Failure must remain a failure even after publication;
report the workspace ID for recovery. No TLA/TLC requested.

### Initial two-workspace verification (superseded topology)

`just seed-lean-workspace` created workspace
`4085b574-1c3c-42a3-aace-c995e5d60e68` at
`http://localhost:3000/g5labs/G5-Lean`, from source commit
`42c55b57f8c53372efbeaf57ec37ad73af43844d`. The independent code repository
retains 2,192 commits; its imported SHA matches the pinned source. API verification
matches all 34 ontology Markdown paths and exact root bytes, with empty docs.
The browser displays Core Ontology and renders the root while compute is stopped.
The existing ontology-verification workspace retains both members. No stack reset,
push or implicit VM startup occurred; the new seed implementation is uncommitted
and therefore intentionally absent from the imported source cut.

Final checks: five real-Git unit cases plus two actual local-stack tests passed
(7 tests, 6.2 seconds). Live cases cover creation, default duplicate refusal,
wrong replacement ID, successful explicit replacement and concurrent same-name
creation with exactly one success. Disposable test workspaces and parents were
retired through the product delete API. Shell syntax, Just recipe parsing,
ontology metadata pairing and `git diff --check` pass. No provider run, application
build or VM resume is needed or claimed for this seed command. The first attempt
failed on imported Git ownership before DB publication; importing under the API
user corrected the failure and the final live tests include that fix.

### Authorized single-workspace correction — 2026-10-14

The human rejected the visible seed parent and authorized one ordinary standalone
G5-Lean workspace, preserving normal future forks. Update command, docs and tests;
then `just dev-down`, `just dev-up`, reseed and verify the browser plus an actual
fork, removing that disposable fork afterward. The reset deliberately discards
old local workspace records. The earlier two-workspace evidence above is historical,
not evidence for the corrected topology. No application fork semantics change.

### Executed Lean seed verification

The corrected command passed five Git checks and three live tests (creation and
replacement refusal/acceptance, concurrent same-name creation, and actual forking
of a standalone seed with repository and ontology assertions). `just dev-down`
removed the old dev containers/volumes; `just dev-up` recreated a healthy stack.
The updated `just seed-lean-workspace` then created exactly one G5-Lean workspace:
`fb7d8476-4c71-483c-9fba-ba40468a64df`, with `cloned_from_id=NULL` and `is_base=false`,
from committed `42c55b57f8c53372efbeaf57ec37ad73af43844d`. Its API verifies all 34
ontology paths and exact root bytes. No seed-source workspace or Git ref is created.

The browser Home page shows G5-Lean as a top-level workspace and its Core Ontology
root renders. A browser-created G5-Lean-fork-check acquired the correct parent and
both repository comparison bases through the real rebase-change-graph API; it was
then deleted normally. The final active list is G5-Lean plus the standard dev-auth
fixtures Base, demo and e2e-test. G5-Lean remains stopped. The worker needed the
existing launchctl supervision because the dev-up command-session child exited;
the supervised standard `just worker-host` is running against the fresh org.
No provider credentials were restored after the authorized data reset. The seed
command/plan changes remain uncommitted and are excluded from the imported HEAD.

## Shared editor simplification — 2026-10-14

Authorized by the [human amendment](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#shared-editor-simplification--2026-10-14).
Governing: [presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation)
and [editor tracing scope](../../ontology/features/documents.g5.md#g5.documents.ontology.tracing).
This amendment supersedes earlier visible kind/ID and shared-editor trace requirements
and their historical verification above.

Remove Source/Drives UI, request state and trace panels from SpecEditor and its shared
SemanticRecordFrame, including Project Docs, Views, saved Markdown explainers and
read-only Markdown frames. Keep the separate All intent logs consumer and backend
JSONL readers unchanged. Retain existing implementation-review controls and question
workflows without an empty relationship column.

The shared frame owns a collapsed Metadata disclosure containing the exact stable ID
for every I/D/Q, even without a valid authored footer. Current-format footer content
is passed into that disclosure, preserving authored links and diagnostics. The header
shows kind only; current-format outlines and linked-block preview chrome omit IDs,
with preview IDs available under Metadata. No title controls or ID mutation are added.
IDs remain unchanged in source, metadata pairing, DOM navigation and Save/delete/undo.

Validate current and existing document formats, read-only question frames, missing
metadata, links/previews, content editing, deletion/undo and implementation-review
controls. Run shared editor regression checks and production build, then verify actual
localhost ontology and document UI at monitor aspect ratio. No additional approval,
new storage model, ledger migration or trace-generation workflow is introduced.

### Shared editor verification

Implemented the amendment in the shared frame, editor and question Markdown renderer.
Current-format footer entries no longer leak IDs into the outline or duplicate them
outside the disclosure on ordinary document tabs. Linked-block preview IDs and delete
dialog IDs are also collapsed under Metadata. Backend readers and All intent logs
remain unchanged.

Executed after the final relevant changes:

- 488 frontend tests passed across 23 editor, Markdown and intent-log suites. Coverage
  includes collapsed IDs, missing footers, current-format Project Docs, read-only
  questions, footer links, preview navigation, edit/Save failures, deletion/undo,
  comments, review actions and retained implementation-review controls.
- Production TypeScript/Vite build passed via `just dev-restart-frontend`; all local
  stack health checks passed. Targeted ESLint has zero errors (57 existing-style
  warnings); `git diff --check` passes. Documents ontology has 101 unique blocks
  paired with footer entries.
- Actual localhost browser verification at 1920×1080: Core Ontology has no Source or
  Drives controls, closed Metadata hides IDs, and opening it reveals the exact ID
  and authored footer links. A disposable fork exercised numbered I/D/Q Project
  Docs and current-format Project Docs; IDs appeared once under Metadata and no
  trace controls appeared. Rich-body Save was read back through the real API with
  IDs and authored titles intact. Deletion/undo restored all three statements and
  returned Save to its clean state. The disposable fork was removed through the
  normal non-forced deletion endpoint (204), and G5-Lean remains stopped/unchanged.

The existing automated ontology browser scenario was updated for the new presentation;
the full scenario was not rerun in this follow-up. The actual browser checks above
cover the changed UI, with component regression tests covering failures and shared
consumers. The Project Doc fixture also displayed the existing unavailable review
metadata notice; this change does not create review ledgers. No commit or push is
part of this implementation request.

## Compact shared statements — 2026-10-14

Authorized by the [human layout decision](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#compact-shared-statements--2026-10-14),
governed by [shared presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation).
The shared statement frame owns compact, transparent document layout across ontology,
document editors and read-only question frames. Small muted Intent/Design/Question
labels sit beside the first line; avoid card fills, heavy borders, header bands and
large badge/rail controls. Keep current document/text-column width rules rather than
introducing a reading-width cap. Tighten statement spacing, place collapsed Metadata
after content, and retain visible question/warning status plus implementation review
access. Current-format statement titles stay hidden; existing non-current title
editing remains available under Metadata without a separate visible header.

SpecEditor owns Delete statement in its existing context menu; remove the permanent
Delete button and shared-frame deletion callback. Right-click or Shift+F10 from the
focused statement resolves that exact record, never a remembered adjacent statement.
The menu supports arrow navigation, Enter/Space activation, Escape/Tab dismissal and
focus return. Retain comments/tag actions and saved selections. Read-only frames do
not offer deletion; malformed/nested current statements show a disabled item with
its reason. The existing confirmation, current-draft revalidation, paired deletion,
undo and explicit Save remain authoritative. No Source/Drives or ID-edit control
returns. IDs and authored headings remain unchanged in file content.

Verify mouse and keyboard targeting for multiple statements, dismissal/focus,
read-only and unsafe-deletion refusal, paired delete/undo/Save, comments and tags,
metadata links, statuses, review access, overflow and unchanged page-width rules.
Run shared editor regressions, production build and actual localhost browser checks
at monitor ratio for ontology and ordinary documents. No new storage, migration,
backend behavior or additional approval is needed.

### Compact layout verification

Implemented in the shared frame and existing editor context menu. Statement bodies
use compact paragraph spacing without reserved blank rows for hidden insert controls.
Existing document-column width rules remain unchanged; no reading-width cap was added.
Keyboard focus enters the menu only after its positioning pass makes it visible,
fixing a timing failure discovered in the real browser that DOM component tests did
not reproduce.

- 492 tests passed across 23 editor, Markdown and intent-log suites after the final
  functional change. Coverage includes exact statement targeting, arrow navigation,
  Escape/focus return, disabled-only menus, read-only refusal, deletion/undo, metadata,
  comments/tags, review access, question statuses and existing editing regressions.
- The final TypeScript/Vite production build passed through
  `just dev-restart-frontend`; local stack health checks passed. Targeted ESLint
  reports zero errors and 58 warnings; formatting and `git diff --check` pass.
  The documents ontology retains 101 unique blocks paired with metadata.
- Actual localhost checks at 1920×1080 covered Core Ontology and ordinary Project
  Docs in a disposable fork. Right-click and Shift+F10 targeted the selected Design
  statement; arrow navigation and Escape restored the correct focus. Confirmation
  deleted only that statement; Undo restored all three statement kinds and returned
  Save to its clean state. An ordinary-document body edit saved through the real API
  with all IDs and authored titles intact.
- Current-format Save after deletion removed the selected statement and its paired
  footer, preserving the other statements, metadata and authored links. The resulting
  stale heading link showed a missing-target message. Metadata keyboard disclosure,
  authored evidence-link preview and Open document navigation worked; the heading
  anchor remained at its target statement and the viewport had no horizontal overflow.
- The disposable fork was removed using normal non-forced deletion. The browser was
  returned to G5-Lean. User workspace content and unrelated seed-workflow edits were
  preserved. Changes are not committed or pushed.

The existing ontology browser scenario now uses context-menu deletion. The complete
automated browser scenario was not rerun; the actual localhost checks above cover
the changed interactions, with component tests covering failure paths and read-only
consumers. The ordinary-document fixture still reports unavailable review metadata;
this layout change does not create review ledgers.

## Integration onto Ben ontology — 2026-10-14

Human authorized integration on `saehan/ontology-editor` in the existing checkout,
preserving `g5/saehan-499-ontology-editor` and carrying all six unique non-merge
commits, including bounded Git reads and Lean seeding. Base is `38046b28`. The target
ontology organization and intent govern; editor commitments stay in documents and
seed commitments move to features/operations. Existing historical evidence above
does not establish verification of this combined branch.

The target authoring format uses explicit stable anchors immediately before I/D/Q
markers, without per-statement headings. Parse standalone anchors and markers as
separate source-backed blocks even without blank lines. A matching unique anchor
belongs to the following statement, never its preceding sibling. Natural section
headings remain visible and outside statement deletion. Anchored statements end at
the next statement's anchor or the next section heading/footer; formal relationships
do not create implicit nested source ownership. Existing heading-associated current
statements keep their protected heading ownership and nested-deletion refusal.

The shared frontend parser owns block boundaries, protected identity bytes and paired
deletions; Rust format facts must agree through shared fixtures. Preserve anchors,
IDs and footer during body editing. Refuse mismatched/duplicate anchor identity,
duplicate markers, ambiguous metadata or malformed footer deletion. A deletion removes
the selected anchor/marker/body and uniquely paired metadata in one undoable draft.
The editor renders the anchor at the matching record, and previews resolve explicit
anchor targets to that record. Do not add inferred relationships, move tracking or
statement creation controls.

Verify shared fixtures for adjacent anchored statements, section boundaries, lists,
Unicode, fenced examples and unsafe identity; test every actual ontology file's
markers, metadata, anchors and safe deletion preservation. Run affected frontend and
backend checks, build and localhost verification using the new committed corpus in
a disposable workspace. No new TLA/TLC campaign is requested.

### Integrated branch verification

The six source commits are replayed on `38046b28`; `df76a43b` adds anchored-format
adaptation and the reconciled design. The original branch remains at `f2377b27`.
No new worktree was created. Documents retains the target's natural sections and
adds the compatible editor/read-bound commitments; seed commitments live in
features/operations, without restoring the deleted concepts/operations page.

- 756 frontend tests passed across 23 affected suites. Shared fixtures cover anchored
  siblings, Unicode, section boundaries, adjacent source lines, fenced/inline examples
  and mismatched/duplicate anchors. Actual-corpus tests resolve every statement and
  verify that each safe deletion preserves all other statements and their metadata.
- 391 contracts and 230 SCM unit tests passed, plus all eight targeted workspace-reader
  tests for the carried security fix. The real current-ontology API integration test
  passed with anchored source, covering discovery, repository identity, live content,
  saving and conflict checks. All eight seed Git/live tests passed, including duplicate
  claims, replacement guards, independent pinned history and standalone forks.
- TypeScript checking and rebuilt frontend/backend Docker images passed; the standard
  local stack health checks passed. Targeted ESLint reports zero errors and 50 warnings.
  Formatting and `git diff --check` passed. Changed ontology has unique paired IDs and
  valid relative links/explicit ontology targets.
- A disposable workspace seeded from `df76a43b` exposed all 20 ontology files through
  the real API. Localhost rendered the target root and all 21 reconciled documents
  statements without format alerts. The code-only sidebar shows files directly.
  Natural headings and compact full-word kind labels remain visible; statement IDs
  appear only after opening Metadata. The 1920x1080 layout has no horizontal overflow.
- Shift+F10 targeted the selected anchored root statement. Deletion removed that
  statement, Undo restored it and returned Save to clean state. A subsequent saved
  deletion was checked against exact API source: only the selected anchor/marker/body
  and paired metadata were removed, preserving siblings, natural headings and separators.
- The target's cross-file Save acknowledgment link previewed only the linked block;
  Open document navigated within the same repository and scrolled to its explicit
  anchor inside the matching statement. A real body edit/Save preserved all anchors,
  IDs, authored headings and the complete metadata footer.
- The disposable workspace was removed with normal non-forced deletion, and the browser
  returned to the retained G5-Lean workspace. Its existing repository content was not
  replaced. Localhost now runs the rebuilt integrated application.

The complete automated browser suite was not rerun; the real browser checks above
exercise the changed anchored-format interactions, with unit/component/API coverage
for failure paths. This does not claim a new chat-agent run, microVM campaign or
TLA/TLC execution. No publication or push was performed.

## Subtle statement boundaries — 2026-10-14

Apply the accepted presentation refinement through the existing SemanticRecordFrame,
covering editable and read-only shared consumers. Use consistent compact vertical
spacing plus a narrow muted left-edge highlight on hover or focus anywhere within a
statement, including its Metadata. Place the highlight outside layout flow so text
width and alignment stay unchanged; retain the existing keyboard focus ring. Keep
normal document backgrounds, full-word kind labels, collapsed IDs/metadata and
context-menu deletion. Natural section headings remain outside statement frames.

The governing owner is [shared presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation),
with [human intent](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#subtle-statement-boundaries--2026-10-14).
Before/after rendering, change highlighting, comparison baselines and unchanged-content
controls are deferred. No comparison state, persistence or new editor controls are added.
Verify shared editor regressions and frontend build, then inspect actual localhost
resting/hover/keyboard-focus states, Metadata access and unchanged content width at
monitor proportions. This visual refinement needs no new behavior tests or model work.

### Boundary refinement verification

Implemented in the shared frame and its stylesheet: compact 8px vertical padding
and a 2px decorative edge outside layout flow, visible on hover or focus-within.
The existing frame focus ring and all controls remain unchanged. Both SpecEditor
and the shared MarkdownRenderer use this frame.

- 248 existing tests passed across six suites covering current ontology, shared
  Markdown rendering, review rendering, structured questions and referenced cards.
  Actual-corpus parsing/deletion preservation checks also passed.
- TypeScript/Vite production build and frontend container restart passed; local
  stack health checks passed. Prettier and `git diff --check` passed.
- Actual localhost checks in the stopped workspace's read-only editor at 1920x1080
  verified transparent resting edges, a muted hover edge, and a keyboard-focus edge
  on a different statement without pointer hover. Metadata opens/closes by keyboard;
  IDs remain inside it. Measured statement/body widths before and after the frontend
  change were unchanged (1538px/1458px); no horizontal page overflow was present.
- User workspace files and compute state were unchanged. No before/after view was
  implemented. The full browser suite was not rerun; this CSS-only refinement uses
  the existing interaction tests plus the browser appearance checks above.

## On-demand statement details — 2026-10-14

The accepted mock supersedes the earlier permanent kind label and Metadata summary.
SemanticRecordFrame owns one independent disclosure per statement, shared by editable
SpecEditor and read-only MarkdownRenderer consumers. Place an accessible information
button at the right edge; hover/focus-within reveals it, and touch or an expanded
disclosure keeps it visible. Remove the leading label gutter and align statement text
with document headings; reserve a small trailing action area to prevent hover layout
shifts. Preserve the overall document-column width, subtle boundary, keyboard focus
indication, context-menu deletion, content editing and review access.
The native button exposes expanded state and controls a uniquely identified details
panel below the body containing Type and ID (the stable identifier), existing title
controls where already supported and authored metadata/links. Toggling details changes
only local UI state.

Question status, severity, Blocking, Uncertain and format warnings remain outside
this panel. A Q without a reliable parsed answer state gets a visible Question label,
not an inferred Open/Answered status. No provenance or review claims change. Reference
popovers retain their existing separate disclosure; no new comparison UI is introduced.

Governed by [shared presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation)
and [human selection](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#on-demand-statement-details--2026-10-14).
Verify independent disclosures, keyboard/touch access, hidden kind/IDs at rest,
visible question/warning states and authored metadata links in editable/read-only
consumers. Run affected regressions, frontend build and real localhost interaction
checks; update the existing browser scenario for the new trigger.

### On-demand details verification

- 490 tests passed across 21 editor, shared Markdown and current-ontology suites
  after the final trailing-action layout change. New coverage checks independent
  disclosures, unique control/panel IDs, Enter/Space toggling in editable and read-only
  views, no Save side effect, Type/ID labels and hidden classification at rest.
  Plain questions, malformed-source warnings, Open/severity/Blocking and uncertainty
  remain visible while details are closed. Existing metadata navigation, review and
  deletion regressions pass. The browser scenario uses the new information button.
- TypeScript/Vite build and standard frontend restart passed; all local stack health
  checks passed. Targeted ESLint has zero errors and six warnings. Prettier, diff
  whitespace checks and the 21-block ontology/metadata pairing check pass.
- Actual localhost checks in the stopped workspace's read-only editor verified hidden
  type/ID and transparent buttons at rest, keyboard-revealed controls, independent
  Enter/Space disclosures, Tab access and authored links. The final 1920x1080 layout
  puts statement content at the frame/heading start (x=350) and the icon at the right
  (x=1864), with no leading label gutter or horizontal overflow. An actual ontology
  question remains visibly labeled while its Type/ID/provenance panel is closed or open.
- User workspace files and compute state were unchanged. Touch visibility/minimum
  target size is implemented through pointer/hover media queries; physical touch was
  not exercised. The full automated browser suite was not rerun. No comparison view
  or new persisted metadata was introduced.

## Quiet outline — 2026-10-14

The human-selected Quiet outline replaces the hover-only left boundary so readers
can distinguish statements from ordinary prose without interaction. The shared
SemanticRecordFrame stylesheet owns a persistent one-pixel neutral outline with
slightly rounded corners and compact spacing. Hover/focus-within strengthens the
outline; no fill, new header or permanent type label is added. Keep decoration out
of layout flow to preserve content alignment, text width and the trailing details
control. The outline encloses the statement's body, statuses and expanded details;
ordinary prose, authored section headings and unrelated editor blocks stay unframed.

The owner remains [shared presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation),
with [human selection](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#quiet-outline--2026-10-14).
Type/ID disclosure, keyboard/touch access, context-menu deletion and visible
questions/warnings retain their existing behavior. Comparison views remain deferred.
Verify existing shared-editor regressions and production build, then inspect actual
localhost resting outlines, prose distinction, expanded details, alignment and
absence of overflow at monitor proportions. No new behavior or storage is introduced.

Verification: all 490 tests in 21 affected frontend suites passed; CSS formatting,
metadata pairing and `git diff --check` passed. The production frontend rebuilt and
restarted successfully on localhost:3000. At 1920x1080, live statements have a
one-pixel neutral outline while neither hovered nor focused; headings remain
unframed. Statement left position (350px), frame width (1538px) and body width
(1506px) match the pre-change measurements. The outline fits within the document
padding without horizontal overflow. Enter/Space opens and closes right-edge
Type/ID details, and the outline encloses the expanded panel. Existing question,
warning and context-menu regressions passed; the full automated browser suite and
physical touch interaction were not rerun for this CSS-only adjustment.

## Muted metadata footer — 2026-10-14

The human selected Muted footer to distinguish expanded metadata from statement
content. Update the shared SemanticRecordFrame used by editable and read-only
consumers: put expanded details in a full-width footer within the quiet outline,
with a thin top divider and theme-aware muted fill. The small Metadata heading,
Type and ID share one compact wrapping row, following the human's refinement;
authored metadata retains its content and links.
Metadata links use a persistent underline and theme-aware link color so their
clickability is apparent at rest; hover/focus strengthens the underline and native
keyboard focus remains visible. This presentation-only refinement preserves targets
and existing navigation behavior.
Hide the redundant document-level Metadata heading and outline item when every
footer entry is already disclosed per statement. Share this visibility calculation
between SpecEditor and MarkdownRenderer; do not change source or hide unmatched,
malformed or ordinary Metadata sections. Cover both consumers and incomplete footers.
The body stays unfilled and its width/alignment is unchanged. Keep the right-edge
information control and independent keyboard-accessible disclosure. Questions,
warnings and review access remain outside the collapsed details. No persisted
content, parsing, navigation or comparison behavior changes.

Governing owner: [shared presentation](../../ontology/features/documents.g5.md#g5.documents.ontology.presentation).
Provenance: [human selection](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#muted-metadata-footer--2026-10-14).
Verify existing editable/read-only disclosure, authored links and question/warning
regressions, production build, and localhost layout with expanded/collapsed details.

Verification: the footer implementation passed 490 tests across 21 affected frontend
suites. After the final single-row refinement, all 93 tests in CurrentOntologyEditor
and MarkdownRenderer passed again. Formatting, metadata pairing and diff checks
passed; the final production build and localhost frontend restart succeeded.
Live inspection confirmed a muted fill and top divider, authored provenance links,
keyboard disclosure, visible questions and no horizontal overflow. Metadata, Type
and ID have the same row position; body width remains 1127px before and after at
the inspected viewport. No document edits were made (Save remains disabled).
Physical touch, narrow-screen visual inspection and the full automated browser
suite were not rerun for this presentation change.

The final metadata-link styling refinement passed formatting, pairing and diff
checks plus a fresh production build/restart. Live localhost inspection of the
authored Ben source link confirmed an underline and link color without hover/focus,
and a two-pixel underline plus visible focus outline during keyboard access. The
authored relative href is unchanged. This CSS-only refinement did not rerun the
component suites or follow the source link to verify its destination.

Redundant-heading verification: 118 targeted editor, Markdown renderer and ontology
tests passed, including fully disclosed, unmatched, introductory and malformed footer
cases. The final production build/restart and diff checks passed. On localhost,
system_design_principles has neither a Metadata section heading nor its outline
entry; the last statement still opens its Type/ID/provenance footer. Save remains
disabled. Source Markdown and the per-statement Metadata label are preserved.

### Information button alignment

The human flagged the trailing information button as vertically misaligned. Live
measurement found its center four pixels below the first text line for both
single-line and wrapped statements. Remove the shared action rail's extra top
padding; preserve the icon's right position, text width and existing disclosure
behavior. Verify both line lengths on localhost after rebuilding.

Verification: production build/restart, CSS formatting and diff checks passed.
Live measurements on two single-line and two wrapped statements show zero vertical
offset between the button center and first line center (previously four pixels).
The button remains 24px wide. This padding-only change did not rerun component
tests or physical touch checks.

### Visible document links

Extend the metadata-link presentation to ordinary document prose, lists (including
Reading paths) and statement content in SpecEditor and shared MarkdownRenderer.
Both consumers opt into the same scoped link styles: dotted underline and
theme-aware link color, solid underline on hover/focus, and existing keyboard
focus. Do not change hrefs, navigation, parsing or source. Verify the actual Reading
paths list at rest after rebuilding; keep application navigation chrome unaffected.

Verification: production build/restart, formatting, metadata pairing and diff
checks passed. After refreshing localhost, all 19 Reading paths links have a
persistent underline; the Core concepts link has the themed link color without
hover/focus and retains its authored relative href. This styling-only extension
did not rerun component tests or revalidate destination navigation.

The human refined the underline treatment using reference-link screenshots:
dotted at rest, solid on hover. Apply the same solid treatment to keyboard focus,
preserving the existing focus outline and line thickness. Verify resting and focused
states on localhost after rebuilding; no navigation or content changes are needed.

Verification: production build/restart, CSS formatting and diff checks passed.
The live `save-before-publication` link in the Git ontology has a dotted underline
without hover/focus, and a solid 1px underline plus the existing visible outline
on keyboard focus. Hover uses the same solid-underline rule; a separate pointer
hover interaction and component tests were not rerun for this CSS-only refinement.

## Reference popup presentation — 2026-10-14

Restore the original reference popup's visual hierarchy in CurrentOntologyReference,
retaining AnchoredPopover positioning/dismissal and current-format resolution. The
header identifies the target document path (repository qualifier when supplied),
with a details disclosure and aligned Open document/Close reference icons. Stable
IDs stay in details. Use explicit theme-aware colors and a bordered, height-bounded
rendered-Markdown preview. An optional muted footer exposes Type, ID and the
selected block's authored metadata with working links. Omit generated Current draft,
Live files and Committed files labels, as requested by the human. Reset
details on target changes. Loading/error states retain the header and container;
Close remains available and unresolved navigation is disabled.

Governing owner: [block previews](../../ontology/features/documents.g5.md#g5.documents.ontology.previews).
Provenance: [human approval](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#reference-popup-presentation--2026-10-14).
Verify resolved preview, independent details and provenance, target changes,
open/close and missing/loading states through component tests; rebuild and inspect
the actual localhost popup. Preserve exact target/repository navigation, whole-file
navigation, source content and link-authority rules.

Verification: all 497 affected tests across 21 files passed, including exact-block
preview/provenance, details reset, loading/error chrome, close/focus and navigation.
The production frontend build/restart passed with all services healthy. On localhost,
the Git ontology reference shows the compact path header and bordered rendered
preview; details reveal Type, ID, current-draft status and authored provenance.
Close restores focus to the source link; Open document dismisses the popup and
retains the selected document/workspace. Save remains disabled throughout. Formatting,
metadata pairing and diff checks passed. Loading/error states were verified in
component tests rather than forced in the live workspace.

Follow-up verification: after removing the three generated content-source labels,
all 22 current-format editor tests passed. The frontend build/restart, formatting
and diff checks passed. Expanded popup Metadata on localhost retains Type, ID and
authored provenance without any generated Current draft/Live files/Committed files
label. Source selection and navigation are unchanged.

## Source-link Back navigation — 2026-10-14

Fix the existing [repository-qualified navigation contract](../../ontology/features/documents.g5.md#g5.documents.ontology.links): an ontology origin must
retain its repository when captured and restored by the shared Back action. The
previous implementation kept the file path and statement ID but dropped branchId
for ontology tabs, causing the repository picker in multi-repository workspaces.
Keep the source repository even when the destination belongs to another repository.
Existing statement scrolling and raw-file/document return behavior remain intact.
Verify the origin round trip and the actual ontology → source file → Back flow in
the two-repository localhost workspace.

Verification: all 36 tests in DataHookWiring and CodeEditorTraceNavigation passed,
including source-repository round trips for ontology/code tabs with a different
destination repository. Formatting, diff checks and the production frontend
build/restart passed. In the two-repository localhost workspace, opening the root
statement's Ben source link and clicking Back restored the original root URL with
its repository parameter and the originating statement in view, without the branch
picker. Save remained disabled; no document content was changed.

<a id="core-statement-comparison--2026-10-15"></a>

## Core statement comparison — 2026-10-15

Implemented and verified in the working tree after the human implementation request,
2026-10-15. No commit or publication is claimed. This section supersedes earlier comparison deferrals only for the
core scope below. No remaining blocking product decision. Human choices are recorded
in [core comparison decisions](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#core-statement-comparison--2026-10-15).
Governing owners: [view and editing](../../ontology/features/documents.g5.md#g5.documents.ontology.statement-changes),
[baseline](../../ontology/features/documents.g5.md#g5.documents.ontology.comparison-baseline),
[matching](../../ontology/features/documents.g5.md#g5.documents.ontology.comparison-matching).

### Scope and settled behavior

- Add one Compare toggle, initially on for each document opened, to the current-format
  Core Ontology editor, immediately left of Human with a divider before the filters. Name the mode Statement changes. It shows the full current
  document; ordinary text/headings remain present but are not compared.
- Always use Diff Graph **All changes** semantics, not its Uncommitted only mode,
  current selection or default. The baseline is the repository's merge base between
  the captured workspace and readable parent heads. Base/standalone workspaces,
  unavailable parents, mismatched repository sets and missing merge bases show an
  explicit unavailable state. No accepted-head fallback or new baseline picker.
- Modified: current block then Before. Added: current block with Added label.
  Deleted: read-only prior block with Deleted label. Unchanged: normal current block.
  Modified/Before versions share one quiet outer frame, with a thin divider and no
  gap between versions. Place their labels in an inset left column aligned with the
  first content line; use very light green for current/added content and red for
  historical/deleted content, with normal-contrast body text. Unchanged statements
  remain unfilled; separate pairs have extra vertical spacing. Preserve the existing
  document-column width, no generated statement titles, trailing details and
  question/warning visibility.
- Current content is the editor buffer, including unsaved changes. Existing editing,
  paired deletion/Undo, Save permissions, version guards and conflict behavior apply.
  Before/deleted frames have no editing, deletion, comments or agent mutation actions.
  Toggling changes only presentation, never the document's parsed block/source array.
- Metadata-only edits do not produce changed markers or a Before block. This includes
  Type-only edits; Type/ID/provenance still show truthfully in the current disclosure.
  When both versions are shown for a body change, each has its own authored metadata.
  No new metadata diff/highlighting UI is needed. Body formatting changes count;
  compare source body text with only line-ending normalization, not rendered HTML.
- Pair unique stable IDs within (repository, file). IDs never match across repositories
  or files. ID replacement appears as deletion/addition. Reordering otherwise identical
  statements does not itself produce a body change marker. No move tracking or redirects.
- Keep current document order. Place deleted blocks before the next surviving baseline
  sibling, otherwise after the preceding surviving sibling, otherwise at the document
  end; retain baseline order among adjacent deletions. Ghost blocks never enter Save.
- This phase compares an opened current ontology file, including added files. It does
  not add deleted-file discovery/history navigation to the sidebar. Missing current
  files retain their existing missing-target behavior. Zero statement changes means
  exactly that, not that prose or metadata is unchanged.

### Existing owners and reuse evidence

`WorkspacePage` currently chooses Diff Graph's default from `clonedFromId`, while
`SourceControlChangeGraphView` keeps its mode in local component state. This feature
must explicitly request workspace/All changes; it does not lift or synchronize that
UI state. `source_control_change_graph_service::project_with_snapshots` rejects Base
workspaces, authorizes the parent, captures repository versions and calls
`select_comparison_bases`. `workspace_repositories::comparison_bases` requires the
same repository membership and selects merge bases; `parent_comparison_bases` also
exists for authorized captured bindings. Reuse that authority and selection logic,
with the same captured inputs; do not copy the merge-base algorithm into the editor.

Graph DTOs expose revision/range and bounded semantic diff hunks, not complete baseline
Markdown for arbitrary unchanged/open files. Hunk output can be truncated and cannot
serve as the complete Before document. Do not fetch/project the entire relationship
graph or reconstruct a baseline from hunks for this view.

Add a bounded, read-only baseline-file operation under the existing authorized
ontology/workspace reading boundary. Input is workspace ID, repository ID and normalized
ontology path; All changes is fixed. The server captures source and readable-parent
repository versions through existing Git/read owners, selects the shared merge base,
and returns the requested file bytes or an explicit absent-file result, baseline SHA,
source/parent revision identity and a comparison revision token. Reuse immutable
committed views (`workspace_reader`) and existing path/access checks. No caller-supplied
commit grants read authority. Retain workspace/parent/member validation even though
only one file is materialized. Missing history, denied access and absence are distinct.
If shared orchestration must be extracted, keep it under the existing Git owner and
exercise the unchanged Diff Graph consumer against the same selection fixtures.

The open-document host owns request lifecycle and comparison identity; SpecEditor
retains the sole editable buffer. A pure helper using the existing current-ontology
parser computes paired body changes from immutable baseline bytes and that buffer.
SemanticRecordFrame/MarkdownRenderer render read-only Before content without placing
it in editable blocks, the editing DOM target map, outline or duplicate anchor IDs.
Current statement navigation continues to select the current block. Historical links
must not silently resolve as historical using live files: for this core phase, Before
links retain readable authored text/target but are non-navigating, with an accessible
explanation that historical navigation is unavailable. Current links work normally.

### Freshness, limits and failure behavior

- Request keys include workspace, repository, path and the observed source revision;
  each immutable response binds its comparison revision, selected SHA and bytes in one
  value. Do not maintain a separate unversioned body cache. Fetch on mode activation and invalidate on the existing workspace/source-control refresh signals
  affecting refs or membership; recheck when refocusing/re-entering the view. Recompute
  local body comparisons as the editor buffer changes without another baseline fetch.
  Recheck parent inputs every 30 seconds while active, since the parent's events may
  not reach a child. Source ref/sync/reconnect events invalidate the existing
  source-control query family, including the baseline operation.
- Pin the baseline bytes and displayed SHA together for each result. Async replies for
  another document/repository or superseded request cannot replace current state.
  When source/parent inputs change, invalidate and coherently replace the comparison;
  retain the same query's last successful comparison during background fetching,
  with an updating tooltip identifying its displayed baseline. Initial loading,
  errors and query-key changes do not borrow another query's baseline. Preserve
  local edits and editor conflict checks throughout. This is a derived read, not an
  atomic snapshot of live filesystem and parent refs, nor a new persisted lifecycle.
- Apply existing reader caps plus a 10 MiB combined baseline/current UTF-8 input limit
  (matching Diff Graph's file-diff input budget) and at most 5,000 parsed statements
  across both versions. Refuse before expensive pairing/rendering; never truncate a
  file and classify the remainder as deleted. Use bounded file reads before loading
  historical blobs. Do not load other document bodies or build workspace graphs.
  The exact-cap test exposed repeated whole-document scans in the shared parser;
  index anchors, IDs, footer entries and section boundaries once per parse, preserving
  the existing parsing/deletion contract rather than increasing the timeout.
- Loading, absent baseline file, unavailable comparison, oversized input, ambiguous IDs
  and malformed structure are distinct states. A confirmed absent file at a valid
  baseline makes valid current statements Added. An unavailable baseline never does.
  Ambiguous IDs/anchors or invalid statement/metadata boundaries disable comparison
  for that document with actionable diagnostics; normal editor diagnostics persist.
- Existing read-only editor contexts may show comparison but remain read-only. Shared
  Project Docs and ordinary Files views gain no new comparison controls in this phase.

### Implementation sequence and acceptance evidence

1. Implement/reuse the authorized immutable baseline-file read and shared selection
   owner. API integration tests compare its selected SHA with Diff Graph All changes
   for identical captured inputs, including divergent parent/source histories.
2. Add pure statement pairing and read-only rendering; connect one toggle to the
   existing editor buffer without adding a second editable document store.
3. Connect refresh/error paths and preserve existing navigation/Save/Undo behavior.
4. Run affected backend, parser/component, frontend build and real browser journeys
   after final relevant changes. Record scoped results here and under governing IDs;
   current prior editor evidence does not validate comparison.

Required cases: modified/added/deleted/unchanged statements; no statements; added
file with absent baseline; all statements deleted from an existing file; interleaved
prose/headings; metadata-only and Type-only changes without flags; own-version metadata;
formatting changes; ID replacement/reorder; deletion placement; duplicate/malformed
IDs/anchors and missing/ambiguous metadata; exact/over input limits without truncation.
API cases include multi-repository equal paths, parent read denial, standalone/Base,
missing refs/merge base, membership mismatch, concurrent parent/source advancement,
old responses after switching files, and unchanged Diff Graph behavior.

Real browser coverage: activate/deactivate comparison; expand details; keyboard use;
edit a body with unsaved changes and see the Before pair; Save/reload and still compare
against All changes (Save does not make it disappear like Uncommitted only); delete/
Undo without serializing ghost content; failed Save and agent refresh preserve drafts;
parent refresh changes the displayed revision coherently; current links/comments target
current content; Before remains read-only; text width and normal editing are unchanged.
Use actual API/file reads for the main journey and controlled fixtures for failure
boundaries. No TLA+/TLC, production proof campaign or new review controls are requested.

Deferred within the editor: prose/headings comparison, changes-only filtering/collapsed gaps, separate
Show before control, word-level/metadata diff highlighting, baseline management,
cross-file matching/move tracking, whole-file deletion browsing, historical-link
navigation, comparison-specific assessments/approvals and dependency visualization.

Design self-review: reused baseline selection, immutable reading, parsing, editor
buffer, Save guards and shared frames; added only a baseline-file read and derived
presentation. No graph projection, durable baseline, duplicate editor state or new
approval lifecycle is needed. All product decisions are settled; implementation must
verify refresh bindings and shared baseline parity rather than assuming those APIs
already return complete historical text. Document validation is recorded below.

Design-stage document validation: 24 unique statement IDs and matching metadata entries remain
paired in order; explicit anchors and affected local design/provenance links resolve.
`git diff --check` passed. No application code, runtime state or implementation
evidence was changed by this design update.


<a id="comparison-implementation-and-verification--2026-10-15"></a>

### Comparison implementation and verification — 2026-10-15

Implemented the baseline read under the existing Source Control Git service and
workspace read authorization; it calls the same `select_comparison_bases` owner as
All changes, then reads one bounded immutable Markdown blob. Responses bind bytes,
selected SHA and captured source/parent revision identities together. Confirmed
absence is nullable content; unavailable authority/history never becomes absence.

The shared editor computes a derived comparison from its existing local buffer,
with no historical bytes in editable blocks, Save, outline, anchor targets or comment
selectors. Before/Deleted frames reuse the shared Markdown/frame presentation and
have their own Metadata disclosure. Their links retain readable targets without
navigation. Current links and comments keep current repository/file identity.
Filters are temporarily disabled while comparison shows the full document. Closing
comparison leaves the draft and normal editor behavior intact.

Source ref/sync/reconnect signals invalidate the baseline query through the existing
WebSocket/source-control refresh owner; key changes, focus, activation and a 30-second
parent recheck also refresh it. Background fetching retains the same query's last
successful comparison and count; initial loading and errors show their status while
preserving local content. No graph-mode state, graph projection or persisted baseline
was added. The exact statement-limit check exposed quadratic parser lookups; indexing
existing anchors/IDs/metadata/heading boundaries keeps parsing semantics unchanged
and lets the 5,000-statement boundary test complete within the normal test budget.

Executed checks in the working tree:

- `cargo check -p g5-api-routes` passed. Three real HTTP/Git baseline tests passed,
  covering independently advanced parent/child histories, All changes parity,
  same-path repository isolation, absent files, changed parent revision, foreign
  repositories, invalid paths, absent/private parents, Base, missing refs, unrelated
  histories, membership mismatch, unauthorized users and oversized blobs. Seven
  existing repository-authority/selection regressions also passed.
- Nine focused frontend suites passed (352 tests): source parsing and protected
  boundaries, matching/deletion placement, Type/metadata/prose exclusions, exact and
  over byte/statement limits, historical metadata/isolation, read-only consumers,
  drafts/failed Save/agent refresh, stale requests across repository/file changes,
  revision refresh, WebSocket invalidation and the existing Diff Graph query hook.
- TypeScript checking passed. Targeted ESLint reports no errors; shared-editor lint
  warnings and React test act warnings are not treated as clean-lint evidence.
- The final localhost:3000 Chromium journey passed on the rebuilt frontend/backend:
  keyboard activation, unchanged prose, unsaved pairing, own-version details, Save and
  reload against the parent baseline, current-link previews, current repository/file
  comments, deletion/Undo and toggling. A real competing file write preserved the human
  draft, Save returned 409, and explicit Reload recovered the agent-written content
  while comparison remained active. Disposable parent/child fixtures were removed.
- Backend/frontend rebuilds and local service health checks passed. TypeScript,
  Prettier, targeted rustfmt and `git diff --check` passed. Final document validation
  confirms 24 unique statements with matching anchors/metadata and affected links.
  Local E2E used the seeded demo owner's test-token authentication after the generic
  OAuth setup persona did not match the existing seed; no production auth claim.

This evidence is scoped to these interfaces and the local stack. It is not a new
production assurance campaign or microVM qualification. Deferred comparison features
remain out of scope.

### Connected comparison presentation — 2026-10-15

Implemented the approved shared frame with inset left labels and subtle green/red
fills. Modified and Before share an outer border and divider; Added and Deleted use
the corresponding single-version treatment. Unchanged statements retain their normal
quiet outlines. The document column width and comparison semantics are unchanged.
Comparison rows use the same inner vertical padding as unchanged statements: frame
margins stay outside the pair, historical Markdown has no leading/trailing block
margin, and inset labels align with the first content line. Multi-line bodies grow
naturally; no fixed statement height is imposed.

Expanded comparison metadata stays within the statement content column, with a
slightly stronger tint matching its green or red version. A top divider separates
the footer from statement content; it does not extend beneath the label column.
Ordinary unchanged statements retain their neutral metadata footer.
All shared statement metadata footers remove the last Markdown block's trailing
margin, leaving the footer's own padding as the bottom gap.

The seven focused comparison component tests, TypeScript check, production frontend
build and local service health checks passed. On rebuilt localhost:3000, the existing
statement-comparison-demo showed the connected Modified/Before pair with aligned
labels, readable body text and unfilled surrounding statements. Historical Metadata
opened and closed with the correct baseline content and read-only notice.

After the spacing correction, the production frontend rebuild passed. Browser
measurements on localhost:3000 confirmed matching 40px content-frame heights and
8px vertical padding for single-line Modified, Before and unchanged statements;
visual inspection confirmed the matching spacing within the connected pair.

The subsequent metadata tint adjustment passed the production rebuild and local
service health checks. Browser inspection confirmed green/red footers remain inset
by the 80px label column, with both disclosures open.
The trailing-margin correction also passed the production rebuild. Browser checks
confirmed zero trailing Markdown margin and retained 12px footer padding in ordinary
and comparison statements; the demo footers shrank from 110.5px to 98.5px.

### Rebase integration — 2026-10-15

Reconciled onto `695df79c` from `origin/ziniuw/ontology_update_based_on_ben`.
Retained upstream reader suggestions, comment placement and discussion draft
contracts alongside the editor and bounded-read commitments. Preserved removal
of obsolete model links and the upstream Rust dependency update. Application
source is unchanged from the pre-rebase editor branch; upstream recovered
contracts do not add new implementation or verification claims here.

Post-rebase checks: 45 tests passed across the current-format parser, comparison
logic and comparison component suites. The two reconciled ontology files retain
26 and 8 unique statements respectively, with matching anchors and ordered metadata;
upstream contracts and editor additions were checked against both pre-rebase trees.
`git diff --check` passed. No new backend build or browser run was performed for
this rebase.


### Files reading typography in the shared editor — 2026-10-15

Human-approved refinement: use Files preview's reading typography, Markdown
formatting and vertical rhythm throughout the shared editor. Page width, maximum
width and horizontal document padding remain unchanged.

- Use 16px body text with 1.5 line height, 32px/24px primary headings, conventional
  heading/paragraph/list spacing, hanging list markers and shaded inline code.
- Remove per-block insertion-control padding and stacked heading styles from the
  editable child. Hover and keyboard insertion controls remain accessible as
  overlays; they must not reserve an empty line for every block.
- Render nested inline formatting (including links inside bold text) in both
  editable and read-only paths. Preserve link security, repository navigation,
  source bytes on untouched fields and formatting when an edited field is saved.
- Keep statement outlines, on-demand details, metadata labels, comments, question
  warnings and comparison colors. Current/Before body typography and padding
  match; metadata retains its distinct footer and has no trailing blank space.
- Scope reading styles to document content, not toolbar controls or unrelated chat
  surfaces. No source migration, baseline change or new editor workflow.

Verification: regression checks for nested formatting and editing round trips,
link safety, shared renderer/editor and statement comparison; frontend build;
visual checks on localhost for ordinary prose, expanded metadata and Current/Before
rows, including unchanged page width and insertion access.

Verification completed: 540 tests passed across 10 focused suites covering inline
formatting/link safety, edit serialization, Save, current ontology, comparison,
shared Markdown, comments, review suggestions and Files preview. The final inline
helper refinement passed the 286-test utils/deep-link subset again. Production
build and formatting/diff checks passed. The local Docker frontend was rebuilt
and its health check passed.

Browser verification on the existing Project Docs test document confirmed 16px
body/24px line height, 32px H1/24px H2, shaded inline code and a rendered link
inside bold text. Its column remained 1486px wide with the same 32px horizontal
padding. Keyboard Tab reached Insert block; viewing/focusing left Save disabled.
Current and Before both measured 24px single-line body height with 8px frame
padding and zero trailing metadata margin; colored inset footers remained intact.


Insertion-position follow-up: place the ordinary block's 20px insertion button
slightly above the midpoint of the existing 16px inter-block gap. The initial
centered position was too low in human visual review; move it 3px up, so its center
is 5px below the text block's bottom (7px below the original position). Preserve
document spacing/width and the separate compact placement within statement frames.
Verification: the rebuilt localhost frontend measures a 5px center offset and
unchanged 16px paragraph gap. Production build and service health checks passed.


### Ontology statement list presentation — 2026-10-15

Present the existing set of statements in ontology files as a normal, compact
bullet list. This supersedes the resting Quiet outline for ontology files only.
Use the existing ontology-path ownership check; Project Docs and other shared
editor consumers retain their current presentation and editing controls.

- Presentation only: preserve statement boundaries, IDs, authored Markdown and
  metadata. Do not convert, merge or split the stored statements.
- Use normal list-item spacing (24px line height, 4px between statements), no
  per-statement vertical card padding and no outline until hover/focus.
- Add a display bullet for plain-text statements. Reuse authored bullets without
  duplication; preserve nested, multi-item, ordered and task-list structure.
  Continuation paragraphs stay aligned with their statement text.
- Clicking an already-bulleted ontology statement opens the authored plain text,
  including its existing `-` (or other authored marker), rather than list-building
  controls. Keep the existing guarded raw-text edit path and exact source on an
  untouched field. Keep the visual bullet outside the editing field and align the
  field with the rendered statement text; the authored marker remains inside the
  plain text. Never insert a presentation-only bullet into the editor text.
- Keep details on demand, visible questions/warnings, comments and editing access.
  Current and Before use the same compact rhythm while retaining inset labels,
  comparison colors and inset metadata footers. Keep page width unchanged.

Verification completed: 46 tests passed across current ontology, comparison and
explicit-save suites. Coverage includes original `-`/`*`/`+` markers surviving edits,
no changes on untouched activation, no duplicate outer bullets for structured lists,
plain-body bullets in Current/Before, and unchanged Project Docs list editing.
TypeScript, formatting and diff checks passed. The production frontend build and
local service health checks passed.

Localhost verification measured 24px statement rows with 4px gaps (previously
40px rows with 8px combined margins), equal Current/Before row heights, transparent
resting outlines and unchanged 1486px document width. Clicking an authored bullet
opened plain text including its `-`; untouched activation left Save disabled.
Both comparison metadata disclosures remain independently clickable with inset
colored footers. Narrowed the ontology insertion-control hit area to its centered
20px control so it cannot intercept text or metadata clicks in the compact rows.
Project Docs has no ontology styling opt-in and retains 16px body type, 16px
paragraph spacing and the same page width. No document source was changed during
the browser checks.

Editing-alignment follow-up: retain a visual list marker outside the authored
plain-text field. Browser verification confirmed the field and neighboring
statement text start at the same horizontal position; the field still contains
its authored `-`, and Save remains disabled after untouched activation. All 31
current-ontology editor tests, the production rebuild and local health checks
passed for this refinement.


### Uniform literal ontology statements — 2026-10-15

Human-approved simplification superseding the authored-list special handling above:
every ontology statement uses one fixed native presentation bullet, independent of
its authored contents. Render statement bodies as literal plain text, including
leading `-`, list syntax, Markdown formatting and links. Clicking opens the same
source text after the same fixed bullet. Adding/removing a leading marker must not
change the presentation bullet's size, position or spacing. Preserve source bytes,
statement boundaries/IDs and conflict protection; this is not a source migration.
Apply the same literal body and bullet to read-only Before. Metadata, ordinary
prose outside statements, Project Docs and other files keep their current rendering.

Verification completed: 47 tests passed across current ontology, comparison and
explicit-save suites; the strengthened literal-list checks passed again in the
32-test ontology suite. Tests cover authored syntax remaining literal, dash
removal saving only the intended source change, ordinary-prose reference previews,
rich metadata, Before rendering and unchanged Project Docs list editing. Production
build, formatting/diff checks and local service health checks passed.

Browser verification in a separate local demo tab confirmed the same native disc
marker and 24px body height in Current/Before; the field and rendered body share
the same horizontal origin, with authored `-` retained. Clicking and leaving an
untouched field leaves Save disabled. Metadata links remain rendered/clickable.
Moved the inner insertion control into the inter-row gap after identifying its
invisible button intercepting center-of-text clicks; pointer activation now opens
the text field. An unsaved draft observed in the user's existing tab was left
untouched; no demo source was saved during verification.

## Ontology insertion menu — 2026-10-15

Approved amendment: supersedes the earlier agents/Files-only creation scope in this
plan. The ontology + menu contains Add Statement and Heading only. Add Statement
immediately inserts an Intent with selected “New statement” text in its existing
inline editor after the fixed bullet; Heading inserts H2 with “New heading” selected.
No popup or extra confirmation step. No type or level selection. Other file menus remain unchanged. Ontology statements have no + control;
headings and ordinary document blocks retain insertion controls.

The shared editor owns the menu profile, inline insertion and focus. A current-format
insertion helper owns source boundaries, a stable `user.<UUID>` ID independent of body edits, and
paired human-provided footer metadata in body order. Insertion inside a record
is promoted to after that entire record; document positions stay before Metadata.
The helper validates the entire candidate and preserves existing record content.
Malformed/ambiguous documents, reserved Metadata headings and structural content
masquerading as statement text are refused visibly. A captured source snapshot must
still match when applying; stale insertion targets are refused without mutation. Failed insertion keeps the
menu open and shows an accessible inline error inside that menu, rather than at
the top of the document. When following ordinary content would be absorbed into
a new statement, explain that boundary limitation and suggest inserting after
that content; do not relocate the insertion or change the file format. Dismissing
or reopening the menu clears its error. Successful insertion closes the menu.

Use an explicit validated insertion through the existing local history and Save
path; ordinary text edits retain their structural protection. Undo removes an insertion.
Undo/redo includes body and metadata together; server version-conflict handling remains
authoritative for persistence. Tests cover exact defaults, positioning, metadata
order, source preservation, refusal, history/save and ordinary-document menus.
Browser verification uses a separate tab to preserve the user's unsaved draft.

Implementation: the menu profile, inline editing and validated insertion helper
are in place. An explicit anchor now terminates a preceding heading-anchored
statement without treating the next record as an ambiguous nested declaration;
paired deletion remains protected for truly nested heading-only declarations.

Verification so far: 81 tests across current-format parsing/insertion, current
editor, comparison and explicit Save pass. Production build passes. The governing
file has 26 unique IDs with matching anchors and metadata in body order. The local
`target/debug/g5d spec validate` binary only recognizes legacy `.semlift/specs`
and checked zero current files; it provides no current-format validation evidence.
Local Chromium verification passed: ontology menu exposes exactly Add Statement
and Heading; adding an Intent shows Type: Intent, a stable ID and human-provided
metadata; Heading produces H2 without a submenu. Each operation undoes back to a
clean, Save-disabled document. Project Docs still exposes its full original menu.
The new form keeps native input history separate from document history and supports
Cancel, Escape and a keyboard focus loop. The final frontend was rebuilt and restarted.
Browser insertions were not saved to the workspace, preserving the user's existing
draft and file version; Save payload and undo/redo round trips are covered in the
component suite, not claimed as a new live persistence check. Screenshot evidence:
`/tmp/ontology-insertion-menu.png`. No commit or push performed.

### Inline insertion correction — 2026-10-15

The human rejected the unrequested modal and approved direct inline insertion.
Add Statement creates an Intent immediately and focuses/selects its initial text
after the presentation bullet. Heading creates H2 with “New heading” selected,
matching the existing Project Docs interaction. Both operations remain local drafts
until explicit Save. The earlier modal/form verification above is historical and
is superseded by inline interaction checks. Project Docs behavior is unchanged.
Verification: 56 focused tests passed across the current editor, insertion helper,
comparison and explicit Save. Tests assert no dialog, selected inline text, the
fixed statement bullet, edits in the Save payload, atomic insertion undo/redo and
unchanged Project Docs options. Production build and document ID/metadata checks
passed. Real-browser Heading insertion selected “New heading” in an H2 and Undo
restored a clean buffer. Browser testing identified that the insertion click could
immediately dismiss the new statement editor; ontology menu clicks now stop before
the outside-click listener. Final browser verification passed: the new statement textarea remains focused with
“New statement” selected (selection 0–13), accepts replacement text inline, and
Undo of the edit and insertion restores Save-disabled state. No popup appears.
Screenshots: `/tmp/ontology-inline-statement.png` and `/tmp/ontology-inline-heading.png`.
The localhost frontend was rebuilt/restarted. Verification stayed in a separate tab;
no test content was saved and the user's open draft was untouched. No commit.

The human subsequently specified `user.<UUID>` for newly inserted statement IDs,
replacing the generated `statement.<UUID>` prefix. Anchor, marker and metadata use
the same ID; existing statement IDs remain stable.
Verification of the prefix change: all 34 current-editor tests passed, including
the generated ID's matching anchor, Intent marker and metadata entry. Production
build and localhost restart passed; ontology metadata pairing and diff checks passed.

### Remove ontology statement insertion buttons — 2026-10-15

The human chose removal instead of relocating the + button to a gutter. Suppress
both the per-statement insertion zone and the inner content-block + in ontology
statements. Keep insertion on headings and ordinary document blocks, with the
existing Add Statement/Heading menu and inline behavior. Other document surfaces
retain their controls. This is presentation only; statement identity, metadata,
editing and Save behavior remain unchanged.

Verification: 42 current-editor/comparison tests passed, including both controls
absent on ontology statements and retained Project Docs controls. Production build,
localhost restart, metadata pairing and diff checks passed. A separate Chromium
tab confirmed zero outer or inner insertion buttons across eight ontology statements,
with six ordinary document insertion buttons retained and Save disabled. Evidence:
`/tmp/ontology-no-statement-plus.png`. No user draft was changed; no commit.

### Insertion error feedback — 2026-10-15

Implemented menu-local accessible errors with failure retaining the open menu.
A preflight boundary check explains when the following list or prose would be
absorbed into an Intent, retaining the existing candidate validation and source
format. Closing/reopening clears the error; successful insertion closes the menu.

Verification: 52 focused tests passed across current editor, insertion and
comparison suites; production TypeScript/Vite build passed. A separate localhost
browser tab confirmed failure and keyboard retry keep the menu open, Save remains
disabled, and Escape/reopen clears the message. No document was saved. Screenshot:
`/tmp/ontology-insertion-error.png`. No commit or push performed.

### Statement click-to-edit caret — 2026-10-15

Carry the clicked position from the literal ontology statement preview into its
plain text editor. Resolve the browser's caret position before replacing the
preview, then apply that source offset on editor activation. Authored punctuation
and line breaks count literally. Keyboard activation and selected placeholder
text for newly inserted statements keep their existing behavior; other document
editors retain their existing activation paths.

The ontology statement textarea has no blue focus outline; the subtle active
statement frame and caret continue to indicate editing.

Verification: 46 focused editor/comparison tests passed, including click offsets
for plain text, authored markers and newlines, and existing insertion selection.
Production TypeScript/Vite build passed. A separate localhost browser tab verified
exact caret offsets 35 and 150 across repeated statement activations, with Save
disabled throughout. Screenshot: `/tmp/ontology-click-caret.png`. No commit or push.

### Ontology-wide text editing appearance — 2026-10-15

Extend the quiet text-editing appearance to headings, prose, metadata and
structured text editors within ontology files. Remove blue text-field outlines
and rings; structured editor frames use the ordinary neutral stroke without
a blue inset accent. Keep button/navigation focus indicators, semantic callout
colors, page width and other document surfaces unchanged.

Verification: 46 editor/comparison tests and the production build passed. Separate
localhost browser checks confirmed headings, prose and diagram text fields have
no focus outline/ring, and the diagram editor frame has a neutral stroke with no
blue inset shadow. Save stayed disabled. Screenshot:
`/tmp/ontology-heading-no-blue-border.png`. No commit or push.

### Statement keyboard continuation — 2026-10-15

Implement the approved [Enter behavior](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#statement-enter-behavior--2026-10-15), governed by
[g5.documents.ontology.creation](../../ontology/features/documents.g5.md#g5.documents.ontology.creation).
The raw statement editor owns key handling: only unmodified Enter with a collapsed
caret at the end of the last body block continues a nonempty statement. Enter in
other positions and Shift+Enter preserve native newlines; composition events and
other modified shortcuts do not create records. An empty statement consumes Enter.

Reuse validated insertion for the new Intent, anchor, user ID and metadata in
one history entry. Commit preceding typed text through existing guarded editing
before continuation; refuse stale/ambiguous targets visibly without creating a
record. A blank-body record uses the same plain textarea when active, since the
Markdown parser emits no body block for empty text. Fill it only while its stable
ID still names an empty, valid record; structural text remains protected. No file
format change or new persistence owner is required. Retain Save, undo/redo,
comparison and other-file behavior. Test save/reload, consecutive creation,
blank/middle/selection/Shift+Enter/composition cases, multiple body blocks,
metadata pairing, conflicts and atomic creation undo.

Implementation check: literal body edits are reparsed and must stay inside their
original statement, preventing heading-like text from ending the statement before
continuation. Rejected edits remain in the focused field with a local error; they
do not create a record. This closes a boundary gap in the reused body-edit guard.

Verification: 352 tests passed across current editor, insertion, comparison,
current-format parsing and shared parser utilities. Tests cover creation/save
payload and remount, consecutive Intent IDs and metadata, preceding text edits,
one-step creation undo/redo, newline/selection/IME behavior, multi-block statement
ends and refusal preserving an invalid draft. Targeted ESLint has zero errors
and the same 49 pre-existing SpecEditor warnings as HEAD. Production TypeScript
and Vite build passed. Statement IDs and metadata order remain paired.

A separate localhost Chromium tab confirmed 8 → 9 records on end Enter, no extra
record on repeated empty Enter, a retained Shift+Enter newline, 9 → 10 on another
continuation, focus on each empty field and 10 → 9 on one Undo. Further Undo
restored the original eight records with Save disabled. No browser content was
saved; persistence evidence here is the component Save payload and remount,
not a new backend persistence test. Screenshot: `/tmp/ontology-enter-continuation.png`.

### Empty statement cleanup on blur — 2026-10-15

On leaving an empty or whitespace-only ontology statement editor, remove the
whole statement and its paired metadata using the existing validated deletion
path. This applies to new blank statements and existing statements cleared by
the user. Compare the expected body against live source before deleting, and
never delete another paragraph in a multi-block statement. The deletion is a
single undoable draft change under ordinary Save. Undo/Redo controls perform their
requested history transition without first adding a competing cleanup entry.
Other file editors retain their existing behavior.

Verification: 90 focused editor, insertion, comparison and current-format tests
passed, including paired metadata removal in the Save payload, atomic restore of
a cleared statement, whitespace-only cleanup and preservation of another paragraph.
Production TypeScript/Vite build passed. A separate localhost browser tab confirmed
8 → 9 on Enter, 9 → 8 on focus-out, Undo restoring the blank record, and another
Undo restoring the original buffer with Save disabled. No browser content was
saved. Screenshot: `/tmp/ontology-empty-statement-cleanup.png`.

### Outside statement bullets — 2026-10-15

Keep the presentation bullet outside the ontology statement frame. Hover/focus
borders and expanded metadata begin after that bullet. Changed statements use
one outside bullet for the shared Current/Before frame, with comparison labels
inside the frame; Added and Deleted follow the same alignment. Literal source,
editing, metadata disclosure and other document surfaces remain unchanged.

Verification: all 55 current-editor and comparison tests passed. Production
TypeScript/Vite build passed. Localhost visual inspection confirmed an outside
bullet with expanded ordinary and Modified metadata, the shared Before row and
Added row. No document content was edited or saved. Screenshot:
`/tmp/ontology-outside-bullets.png`.

Metadata refinement: remove the unfilled side gutters around expanded metadata.
Ordinary metadata fills the statement frame; comparison metadata reaches the
right edge while retaining the label column. Match metadata separators and the
expanded ordinary frame outline to the metadata fill. Keep content padding.
Remove the metadata top margin as well: opening details starts the panel
directly below the statement, without introducing an empty strip.

Refinement verification: production build passed. Browser measurements show
zero gap above metadata and zero side gaps to the frame; outline and fill both
resolve to rgb(240, 242, 247). Screenshot: `/tmp/ontology-metadata-flush.png`.

The Modified/Before divider follows the shared frame hover/focus state: it is
transparent at rest, visible on hover or keyboard focus, and reserves its pixel
so the rows do not shift.
Divider verification: production build passed; browser computed styles
confirmed a transparent divider at rest and the stroke color on focus, with
1px width preserved. Screenshot: `/tmp/ontology-divider-at-rest.png`.

### Compare toolbar refinement — 2026-10-15

Rename Show changes to Compare and position it immediately left of Human, with a
small divider separating the view toggle from filters. Default on when opening
an ontology document; keep the toggle visibly selected while enabled. Turning
it off applies to the current view; switching documents resets the default.
Keep current filter mutual exclusion and baseline-unavailable behavior. This
request does not introduce combined filter/comparison semantics. Other document
surfaces remain unchanged.

Verification: 56 current-editor and comparison tests passed, covering default
on, current-view opt-out, document changes, toolbar order, non-ontology surfaces
and existing editing/comparison regressions. Production TypeScript/Vite build
passed. Localhost confirmed Compare selected on load, Before present without
interaction, and turning Compare off removing Before and enabling Human. No
document edits or saves. Screenshot: `/tmp/ontology-compare-toolbar.png`.

### Statement group insertion — 2026-10-15

Show one + after each visible consecutive group of current ontology statements,
following its final Current/Before pair and any attached historical deletions.
Keep it hidden at rest, visible on insertion-area hover or keyboard focus, like
the existing insertion controls. Center it on the document column, without
the statement bullet indentation, to align with heading/prose + buttons.
Reuse the existing Add Statement (Intent) / Heading (H2) menu, inline focus,
validated source insertion, metadata pairing and draft history. Anchor to the
last current statement's stable ID. Keep per-statement controls absent and do
not add controls to read-only editors or historical-only groups. Project Docs
and other files retain their existing insertion UI.

Verification: 69 tests passed across current editor, comparison and insertion,
including two statement groups, both menu actions, correct source placement and
metadata pairing, read-only/Project Docs exclusion and control placement after
Before. Final production TypeScript/Vite build passed. Localhost confirmed one
control for the group, inline focused creation and Undo restoring Save-disabled
state without saving. Final browser measurements confirmed opacity 0 at rest,
opacity 1 on area hover without focus, and the same x center (799px) as all six
heading/prose insertion buttons. Screenshot: `/tmp/ontology-group-insert-final.png`.

### Comparison count in toolbar — 2026-10-15

Remove the summary above the document and show the statement-change count inside
Compare while enabled. Show zero when comparison succeeds with no changes; use an
ellipsis while loading and an unavailable indicator on failure, never a false zero.
Keep baseline details and failure reasons in the button tooltip and accessible
status. Turning Compare off hides the count. The document starts with its own content.

Verification: eight comparison tests passed, including numeric/zero counts,
loading, unavailable and off states. Production TypeScript/Vite build passed.
Localhost shows Compare with count 2 and no summary above the document; baseline
remains in its tooltip. Screenshot: `/tmp/compare-toolbar-counter.png`.

### Stable background comparison refresh — 2026-10-16

Retain the current query's successful baseline while refetching so unchanged
comparison rows, metadata disclosure and counts do not disappear and reappear.
Continue comparing the current editor buffer to those retained baseline bytes.
Replace results coherently when the request succeeds; identify the displayed SHA
in the updating tooltip. Keep existing unavailable behavior on failure and initial
loading without data. Query-key isolation still prevents reusing another file,
repository, workspace or source revision's baseline. No extra cache is introduced.

Verification: 62 tests passed across comparison UI, baseline query identity and
current ontology editing. A deferred refresh test verifies the identical Before
DOM element, expanded metadata and count remain mounted until the new revision
arrives; failed-refresh coverage verifies unavailable behavior. Production
TypeScript/Vite build passed. Localhost rendering was inspected with historical
metadata expanded. Continuous browser sampling timed out, so refresh continuity
evidence is the controlled component test. Screenshot:
`/tmp/ontology-stable-comparison-refresh.png`.

## All ontology changes — 2026-10-16

The selected Continuous page adds View all changes above the Core Ontology tree.
It opens one read-only workspace page grouped by repository and ontology file,
hiding repository headings for one qualifying repository. Reuse current comparison
frames, outside bullets, inset Added/Modified/Before/Deleted labels, own-version
metadata, and restrained fills. Each modified pair counts once. Open file navigates
to the existing ontology editor with exact repository/path identity; wholly deleted
files stay visible with a deleted-file label and no unavailable editor action.

The [human decision](../../chat_intent/saehan-499-ontology-editor/core-ontology.md#all-ontology-changes--2026-10-16)
settles read-only and saved workspace content, including live uncommitted files.
Unsaved browser buffers remain local to editors and are explicitly excluded.
The existing baseline and matching contracts govern: All changes merge bases,
no baseline fallback, same-file stable IDs, body-only changes. This supersedes
changes-only deferral for this dedicated overview only. No Save, insertion, filter,
review state or draft owner is added.

Owners and capture: the source-control service captures authorized repository,
source, parent and merge-base revisions together. A read-only route combines
baseline paths with actor-aware current ontology discovery and bounded file reads.
Current Files/ontology source selection remains authoritative; the existing graph
projection is not reused because its current overlay input is empty. Recheck
repository refs, current discovery and runtime identity before returning, refusing
source changes or partial discovery. Live file reads are sampled during the request,
not a transactional multi-file snapshot; later writes arrive on refresh.

Read at most 500 union ontology files and 100 MiB of aggregate baseline/current
input, with the existing 10 MiB combined comparison bound and 5,000 declarations
per file. Historical listing retains Files tree limits. Refuse capacity overflow
explicitly, never silently omit files. Parse with the existing frontend comparison
owner; malformed files remain named errors, with totals marked incomplete. No
matching across files or baseline-dependent link navigation is introduced.

The query owns complete response replacement, shares source-control invalidation,
and refreshes every 30 seconds while the overview is open. The sidebar badge
observes that cache without initiating file-body reads; it appears after a comparison.
Keep same-workspace successful rows and open metadata mounted during a pending
refresh; failed refresh reports unavailable.
Keep the file count and baseline text stable during refresh. Indicate pending
refresh only on the refresh icon, with accessible busy state and reduced-motion support.
Never borrow another workspace's cache. Distinguish loading, successful empty,
unavailable baseline/read, malformed file and updating states.

Implemented and verified in the working tree. Production TypeScript/Vite build,
40 comparison/view/navigation tests, 25 current-ontology corpus tests, and the
targeted WorkspacePage sidebar/direct-route integration test passed. New frontend
files pass lint. Controlled refresh tests retain the identical Before element and
expanded metadata; failed refresh, workspace isolation, malformed files and empty
results remain distinct. Sidebar-only mounting starts no corpus request.

Backend compilation passed. Five baseline/overview HTTP integration tests passed,
covering committed and live edits/additions, deleted files, multiple repositories,
wrong-organization access, unavailable parent/runtime and oversized content. A
separate concurrent-parent-revision test passed: advancing the parent while live
reads are pending refuses the assembled response. The runtime transport is a test
double; HTTP routing, database authorization and Git history use real test fixtures.
The route's independent aggregate file-count and byte-budget boundary test passed.

Browser verification used an isolated Vite preview with the real application shell
and a fixture response for the new overview endpoint. Before metadata expanded,
there were no editable controls, and Open file preserved the exact repository.
Screenshot: `/tmp/ontology-all-changes-implemented.png`. The existing localhost:3000
Docker application was not rebuilt, so this is not an end-to-end deployment check.

The static route audit still has two pre-existing failures: captured-reader
recognition in `ontology_format::read_ontology` and secondary-identifier manifest
recognition in older ontology/baseline handlers. All named handler bodies were
confirmed unchanged from HEAD; the new endpoint's HTTP authorization tests pass.
These audit failures remain unresolved and are not claimed as passing evidence.

Live rebuild follow-up: Files returns an authoritative empty live listing without
a checkout OID when a repository has no ontology directory. Accept that empty
inventory in the overview; continue requiring revision evidence for nonempty
inventories and retain the final inventory, runtime and baseline rechecks. This
allows a normal docs repository without ontology files alongside the code repository.
All seven baseline/overview HTTP tests passed, including the new empty live
repository regression and concurrent baseline-change refusal.
Rebuilt localhost:3000 and verified the actual running demo workspace shows four
changes in `root.g5.md`, with the Core Ontology entry selected. This live check
supersedes the earlier deployment gap for this page. Screenshot:
`/tmp/g5-view-all-changes-localhost.jpg`.

Overview presentation refinements: the top bar says “Includes all saved changes,
but not unsaved edits.” Remove the duplicate subtitle and keep count/baseline on
one compact line. File headings use ontology-relative names without `.g5.md` or
`.md`, such as `root` and `concepts/data_model`, without a separate path line.
Seven view tests and the production build pass; deferred-refresh coverage verifies
the count stays visible while the refresh icon is busy, with no updating-text swap.

## Main integration — 2026-10-16

The human authorized the six-step integration plan: back up, reconcile ontology,
rebase only the four editor commits onto main, verify, push with an explicit lease,
and create a main-targeting PR. Merge remains subject to approval.

Pinned upstream: `31ac2da0104d1516e96a4ef6ee41e254c7fa7ec8`.
Original feature head and expected remote lease:
`743a573355ea0f99825226a9a0066393d33cf24c`.
Backup: `backup/ontology-editor-before-main-rebase-20261016`.
Replay boundary: `695df79c`, excluding the older ontology history rewritten before
PR #500 merged. Keep four commits in the resulting branch.

Reconciliation retains main's review-placement/alternatives, reader discussion
authority, bounded reads, explainer and meeting decisions, delivery alerts and
workload-image contracts. Add the editor and local-seeding owners without duplicating
`g5.documents.read.bounds`; main already carries the bounded reader implementation
and newer Rust dependency fix. No old TLA artifact removal or older ontology commits
are replayed. Body IDs and paired metadata remain in matching order (32 document,
11 operations commitments), including every upstream ID.

SpecEditor retains upstream boundary review cards and comment remeasurement alongside
statement comparison. The consolidated API integration target registers the new
baseline/overview module and shares its existing helpers. Update the static security
audit to follow current-node discovery and the actor-aware single-file reader,
plus exact workspace/repository baseline selection, instead of the removed legacy
corpus expansion. An explainer navigation test now supplies current ontology file
identity. Reconcile the presentation contract with the already-approved ontology-only
bullet and hover-border behavior; no new product decision is introduced.

Rebased validation: production TypeScript/Vite build and `cargo check -p
g5-api-routes --locked` pass. Frontend checks pass: 131 ontology/editor tests,
402 Markdown/link/event/data-hook tests, and 425 review/page/comment tests; the
remaining page test initially failed because its fixture omitted file identity,
then passed with the corrected fixture (alongside the overview route test).
All 39 ontology HTTP integration tests pass, including seven baseline/overview
cases. The static route authorization audit passes all 13 tests, following both
workspace and node delegation. These results supersede the earlier two static-audit
failures recorded above. Seed unit checks pass (5; 8 live cases skipped this run).
No browser/live-stack rebuild was performed for this integration. Existing local
Alpine rootfs fixtures became untracked under main's ignore rules and were left intact.

Final browser-test review updated the two existing real-stack scenarios to the
approved default-on Compare toggle and literal statement textarea. Link navigation
is exercised from ordinary prose/metadata, while statement text retains authored
Markdown literally. Playwright discovers both scenarios for Chromium, Firefox and
WebKit; this is syntax/discovery validation, not a newly executed browser flow.

### PR #606 UI regression fixes (2026-10-16)

The G5-Lean seed UI run exposed two editor regressions. Project explicit
`human-provided` and `human-verified` bullets from each uniquely paired metadata
footer into the existing Human filter; source descriptions, code examples and
another statement's metadata do not establish human provenance. Deletion consumes
the deleted statement's trailing separator rather than appending it to the previous
statement. Keep exact body comparison, including authored whitespace and multiline
formatting, and preserve surviving source bytes. These fixes stay within the
existing editor and comparison contracts; the post-merge Git publication failure
is outside this change.

Focused parser, deletion/comparison and editor regression checks pass (106 tests),
as does the production TypeScript/Vite build. In-app browser verification uses
the fresh `pr606-fixes-parent` G5-Lean seed (20 files at `159e9035c`) and its
`pr606-fixes-child` fork with the rebuilt frontend. Human shows all seven
human-provided root goals. Deleting `g5.purpose.enterprise-integration` produces
one Deleted row and no Before/Modified rows; Undo returns the count to zero and
Redo to one. After Save, All changes shows exactly one change in `root`, the
deleted statement, with no false modification of `g5.purpose.thin-client`.

Risk/Question follow-up: include current-format Q statements without requiring a
legacy structured-question payload, and project explicit `uncertain` metadata
flags separated by `·`, `;` or `,`. Parse whole flag-only bullets from the paired
footer so source descriptions and code examples do not become tags. Combined
human provenance remains available to the Human filter. Count each matching
current-format statement once, including Q statements also marked uncertain;
retain legacy document question behavior. Verify real G5-Lean organizations and
users questions in the existing seed, plus isolated Q-only and I/D uncertainty
cases, before committing.

Risk/Question validation: production TypeScript/Vite build and 152 focused tests
pass, including the legacy Project Docs uncertain-tag behavior. In the rebuilt
in-app browser on `pr606-fixes-child`, organizations shows exactly three questions
with semicolon/comma metadata and users shows one with middle-dot metadata.
Through Files, temporarily removing `uncertain` kept the users Q visible (zero
uncertain, one question); restoring `uncertain` and temporarily changing Q to I
kept the Intent visible (one uncertain, zero questions). Restored the original
users source afterward and confirmed Compare reports zero changes for that file.
The badge counts each Q-plus-uncertain statement once.

### PR #606 confirmed CI regression repair (2026-10-19)

Repair the locally reproduced failures without changing product behavior or CI
gates. Remove unsupported `exact` options from eight Testing Library role queries
(string accessible names already match exactly), and classify `ontology_changes`
as not requiring the shell's content branch in the exhaustive tab test.
Update current-reader structural guards to follow current ontology discovery and
actor-aware file-read delegation. Extract affected function bodies with the existing
Rust parser so removed section comments cannot break the guards. Retain legacy
service checks, guard transaction closure before discovery I/O, and check current
routes for projection-table reads and unintended DB scopes.

The original PR head `17d326355` has nine full-scope TypeScript errors and two
current-reader contract failures; the same checks pass on base `31ac2da010`.
GitHub reports failure in `frontend-build` and `rust-xfs-orchestrator`. Cloud Build
logs remain unavailable: refreshed user authentication succeeds, but QA build and
log-view access returns `PERMISSION_DENIED`. Local reproduction establishes these
branch regressions, not the complete set of Cloud Build failures.

Validation: full frontend `tsc -b` and production TypeScript/Vite build pass;
all 176 tests in the five affected frontend suites pass. Frontend lint passes
with 550 warnings and no errors. The consolidated API integration runner passes
all three current-reader contract tests and all 13 route-authorization tests.
Prettier passes for changed frontend tests. The Rust formatting check found an
existing unformatted block in `api_ontology_baseline.rs`; after formatting it,
`cargo fmt --all -- --check` and `git diff --check` pass.
No browser or full Cloud Build execution is claimed by these local checks.

### PR #606 frontend guard and consumer repair (2026-10-19)

The next E2E build passed lint/build and failed at `frontend-test`. Local execution
of the CI coverage command found ten branch-introduced failures and one inherited
guided-journey failure; the six affected suites on base `31ac2da010` reproduce only
the guided-journey failure. Cloud log access remains denied, so these are locally
established causes, not a complete Cloud-log diagnosis.

Implement the requested fixes 1–4: give the seven new raw buttons stable test IDs,
gate outline suppression on `focus-visible` with the existing replacement ring,
use file-backed selection/search fixtures, and exercise refreshed inline trees
through expand/collapse while checking stale children disappear. Review-suggestion
tests assert one record with the original stable ID and retain their card/preview
assertions instead of counting strings in collapsed metadata. Product navigation
and review behavior remain unchanged. The inherited guided-journey failure is
explicitly outside this repair.

Validation: all 130 tests in the five affected suites pass. The full frontend
CI coverage command reports 5,055 passed and one failed across 314 files;
the only remaining failure is the unchanged guided-journey assertion in
`WorkspacePageAlignment.test.tsx:3885` (expected `done`, received `todo`).
Thus the full suite remains red, while all ten failures in the requested scope
are resolved. Full `tsc -b`, production TypeScript/Vite build, changed-file
Prettier and `git diff --check` pass. Lint passes with 550 warnings and no errors.
No live browser or Cloud Build success is claimed by this local validation.

### PR #606 Rust route and branch-ref repair (2026-10-19)

Cloud Build `119c2fa0-1dd0-4cdd-b973-09a309390b7c` on `da14e9543`
confirms two failures: the file-level route scope guard cannot see the overview
registration in another module, and the branch-ref ontology test still creates a
legacy SemLift fixture. These trace to branch commits `159e9035c` and `ce317bb7b`.
Standard project logs are readable with the existing login; the dedicated log
view remains permission-denied. The same head's frontend CI passed all 5,056 tests.

Keep `.g5/ontology` visible in Files, as explicitly confirmed by the user.
Move the overview's existing audited read-scope registration into its handler
module and merge that router at the same source-control URL. Preserve the route
guards, workspace authorization, and response behavior; register the module in
the explicit route-authorization audit. Extend the HTTP coverage to reject a
write-only credential, retaining the cross-tenant rejection check.

Use current-format Markdown in the branch-ref test, locate the document by its
repository path within the tree, and assert it appears exactly once with the
correct repository and document type. Preserve commit/ref authority checks and
verify rewinding the ref restores the reported OID and empty ontology tree.

Validation: all five route guard tests pass. All 32 tests selected from
`git_refs_authoritative`, `api_ontology_baseline`, `route_auth_static`, and
`current_reader_contract` pass, including both previously failing tests and the
new missing-read-scope assertion. The first sandboxed integration attempt could
not connect to local Postgres; the rerun with local database/network access passed.
`cargo fmt --all -- --check` and `git diff --check` pass. Files visibility policy
is unchanged. Full Rust CI and browser tests were not rerun locally.

### PR #606 Clippy repair (2026-10-19)

Cloud Build `bb019d65-6f8a-4fb5-875a-5790a234141e` on `f528b7038`
passed every Rust orchestrator test lane and failed the final gate on
`RUST_CLIPPY`. Its nine diagnostics trace to the earlier `ce317bb7b` migration:
three helpers lost production callers, five borrows became redundant when Files
listing was extracted with reference parameters, and live listing gained an
eighth argument. The CI step records its failure for the final gate, so a green
individual Cloud Build step does not establish that Clippy passed.

Remove the unused projected-node resolver and its import. Compile the two
legacy tree test helpers only in test builds, preserving their regression tests.
Pass existing state/auth references directly and group live listing's path,
depth, and missing-root policy in a private options struct. Keep authority,
listing limits, fallback behavior, and ordinary Files versus ontology missing-root
semantics intact. Keep `.g5/ontology` visible in Files and add its directory and
document paths to the existing visibility matrix. Do not suppress warnings or
change CI gates.

The complete Linux Clippy run then reached previously blocked crates and exposed
five more diagnostics in branch-touched code. Remove a stale test comment and
unused test import/helper, use a boolean assertion directly, and move the current
ontology path-to-node function before the test module without changing its body.
The current-reader guard now identifies the committed listing call independently
of its argument borrowing syntax, retaining the transaction-before-I/O check.

Validation: `cargo clippy --workspace --all-targets -- -D warnings` passes in
the existing Linux `g5/base-rust-warm:26.04` image (Rust 1.97.1), with this checkout
mounted read-only. The native macOS Rust 1.95 run was blocked by 15 unchanged
Linux-isolation dead-code diagnostics, so it is not claimed as a passing run.
All 19 selected unit tests pass: route guards, legacy tree assembly, and path
visibility. Across focused runs, 37 distinct integration/contract tests pass,
covering live Files listings and bounds, current ontology discovery and content,
overview authorization and missing roots, branch-ref authority, semantic-route
authentication, route authorization, and reader transaction ordering. The
reader guard initially failed on its old borrowing marker; a fresh rebuild after
the marker repair passes all three reader tests. Rust formatting and whitespace
checks pass. No warnings were suppressed, and Files visibility policy is unchanged.

### PR #606 insertion comparison and seed storage repair (2026-10-19)

The real G5-Lean seed UI run reproduced two failures: inserting a statement
changed the preceding statement's separator bytes and falsely marked it Modified;
after merging a child, saving in the parent was rejected because the seed's
server repository had a checked-out parent branch and an index from before merge.

Keep the exact-body comparison contract. Insert at the owning statement's existing
boundary without prepending separator bytes to that statement; ordinary prose
insertion still supplies needed separation. Apply this to Intent, blank keyboard
continuation and H2 insertion. Add comparison regression coverage for insertion
between statements, before a heading and before the metadata footer.

Stage both dedicated seed repositories as bare Git stores with explicit parent
and main refs. Preserve the pinned full committed history, empty docs, independent
objects and exclusion of source working changes. Do not migrate or reset existing
seeds. Add a real-Git regression that advances the parent ref as a server merge
does and then publishes another parent commit. Verify the two user flows through
the in-app browser with a fresh G5-Lean seed before commit and push.

Verification: rebuilt the local API and frontend successfully. The insertion and
comparison library suites passed 28 tests; editor/comparison component suites
passed 62 tests. Seed Git tests passed 6 tests, with 9 live-stack cases skipped.
The real-Git regression covers a server-style parent-ref update followed by push.

In-app browser checks used a fresh `pr606-repair-parent` seed pinned to
`e46b7dd7d5105a9d70e7d24f30e6ededca6bc585` and its `pr606-repair-child` fork,
with the rebuilt application including these fixes. Enter after the models
statement and between existing statements created exactly two Added entries,
with no false Modified entries, before and after Save/reload. View all changes
listed only those two additions. The local test PR merged successfully into the
parent. A subsequent parent edit saved successfully, survived reload and left
Source Control reporting no dirty files. Both test workspaces were stopped.
Screenshots: `/tmp/pr606-repair-insertion.png`, `/tmp/pr606-repair-merge.png`,
`/tmp/pr606-repair-postmerge-save.png`. Existing non-bare seeds were not migrated.

### PR #606 additional UI story verification (2026-10-19)

Verified through the in-app browser against PR head
`27f2175eb68e559cca1d02740daadd7bbd25163b`, with fresh committed G5-Lean seed
`pr606-stories-parent` and child workspaces `pr606-stories-child` and
`pr606-stories-conflict`. GitHub reported base
`31ac2da0104d1516e96a4ef6ee41e254c7fa7ec8`. Shell actions prepared only seed,
temporary deleted-file/filter fixtures, and read source/history for attribution;
user-flow assertions came from the UI. No implementation changes were made.

| Story | Observed result |
| --- | --- |
| Clean rebase | Passed. Parent first-goal edit survived; child models and nested repo edits survived; overview showed exactly the two child changes against new baseline `cc44467c24c7`. |
| Conflicting rebase | Passed on the later UI retry after credential setup and updating the installed Claude CLI to pinned version 2.1.285. Both parent and child test sentences survived in the same Intent; the UI reported up to date with the parent. Save/reload, metadata, Compare toggle, and all-changes overview passed against baseline `cc44467c24c7`. Initial attempts were blocked by missing credentials and then the older CLI rejecting `--permission-prompts`. |
| Delete and Undo | Passed for data. Deleted history retained metadata; current record disappeared. Saved deletion removed both declaration and paired metadata in Files preview; Undo plus Save restored each exactly once. See save-label issue below. |
| Multi-file overview | Passed. Correct `root`, `concepts/data_model`, and `features/operations` groups. Deleted operations fixture contributed 11 historical records. Total rose from 12 to 13 only after root draft Save; no duplicate records. Open file navigated to the correct nested ontology. |
| Unsaved versus saved | Passed. Root draft was absent from overview while nested saved change was included; root appeared exactly once after Save/refresh. |
| Background refresh stability | Passed in sampled UI observations spanning 66 seconds: draft text, focused caret at offset 1, editor top position, expanded metadata and visible comparison remained stable while another file was saved. This was not a continuous frame-by-frame blink measurement. |
| Human and Risk/Question | Passed. Human included human-provided and human-verified records and excluded agent records. Metadata-only fixtures on real users ontology yielded one Q without uncertain metadata plus three D records using `·`, `;`, and `,`; source prose mentioning uncertain was excluded; badge count was 4. |
| Rejected insertion | Passed before both ordinary prose and links list: menu-local explanatory error, menu stayed open, no content or comparison-count change. |

Additional UI defect: after Save, Undo/Redo changes the buffer to unsaved but the
enabled save button can continue to say **Saved**. Clicking it still saves, and
reload verifies persistence. Source attribution: the base already has the same
`handleUndo`/`handleRedo`, dirty-state publication without resetting save state,
and save-label conditional in `SpecEditor.tsx`. This is pre-existing by source
comparison, not an A/B browser run on the base build. Credential validation in
`agent_launch.rs` and `config_service.rs`, the conflict choice modal, Codex device
auth UI, and the relevant workspace launch handlers are also unchanged by the
PR. Neither observed issue is attributable to its unique commits.

Temporary filter/deletion fixtures were restored; test compute was stopped;
Source Control showed no dirty files. Screenshots:
`/tmp/pr606-stories-clean-rebase.png`, `/tmp/pr606-stories-saved-overview.png`,
`/tmp/pr606-stories-filters.png`, `/tmp/pr606-stories-insertion-error.png`,
`/tmp/pr606-stories-conflict-blocked.png`,
`/tmp/pr606-stories-stale-saved-label.png`,
`/tmp/pr606-stories-final-overview.png`.

Conflict retry evidence (same G5-Lean seed, 2026-10-19): the UI-launched Claude
workflow reported rebased code commit `fb44b415`, preserving both `PR606 parent
version.` and `PR606 conflict child version.` in `g5.purpose.enterprise-personas`.
Independent editor observations confirmed both sentences, the stable Intent ID,
`human-provided` metadata and Ben line 5 source. A temporary sentence was added
through the editor, saved, and verified after full browser reload. Compare showed
one Modified statement with the parent's text under Before; toggling Compare off
removed only the historical presentation. The read-only overview showed one
changed file (`root`) and one statement against `cc44467c24`. The temporary sentence
was then removed through the editor and saved; a second reload confirmed the
resolved text remained and Source Control showed no dirty files. Test compute
was stopped. Evidence: `/tmp/pr606-conflict-save-reload-passed.png` and
`/tmp/pr606-conflict-resolution-passed.png`. No implementation files changed or
PR branch commits/pushes were made for this retry.


### PR #606 browser fixture compatibility repair (2026-10-19)

Cloud Build `be9edb75-e2e5-45a5-9b6f-d5189e1f5702` on `27f2175eb`
failed the content-loading and hierarchy browser tests. Current discovery reads
`.g5/ontology`, while the shared seed still supplied only legacy `.semlift/specs`
files and SQL projection rows. The new current-format browser journey passed.

Add four committed current-format seed files (root, parser, nested lexer and
engine) with stable I/D/Q records and metadata. Retain legacy projection and
grounding fixtures for their existing consumers. Fail the seed unless the real
ontology API discovers each current file exactly once in the code repository
and returns its exact bytes after workspace startup.

Select the seeded root by API file identity, rather than clicking arbitrary
folder rows. Assert the supported Compare control remains named and operable;
exclude only that control from the retired-diff assertion and require it to be
absent in Project Docs and raw-code editors. Verify `.g5/ontology/root.g5.md`
remains visible and readable through Files. Align hierarchy/editability tests
with folders and current statement bodies, and require all four leaves to open.
No application behavior, visibility policy, or CI gate is changed.

Validation: the shared seed's exact discovery/content checks pass. Real-stack
Chromium runs pass all 13 selected scenarios: five content-loading tests, the
current-ontology journey, the existing ontology explicit-Save test, and all six
demo-smoke cases (plus three authentication setups). Logs:
`/tmp/pr606-e2e-mvp-content-final.log`, `/tmp/pr606-e2e-mvp-save.log`, and
`/tmp/pr606-e2e-mvp-demo-final.log`. Shell syntax, Prettier and whitespace checks
pass. No test was skipped or assertion weakened in the final selected runs.

Local setup used a fresh `pr606-e2e-mvp` fixture and the supported test-token
login; the OAuth stub returned 401. The one-slot development host required the
seed to be stopped during clone-based tests and resumed for the smoke suite.
Only the two newly seeded repositories needed an ownership correction to match
the local root-run API; interrupted clones were removed before the clean runs.
These are local setup limitations, not claims of Cloud Build parity. The
separate parent/child comparison suite was not included in the final runs;
Compare rendering/toggling and current-format Save were verified above.

### PR #606 legacy title visibility repair (2026-10-19)

Restore legacy record titles to the always-visible body of the shared statement
frame. Keep the existing title editor and explicit Save behavior, including human
authorship tagging. Metadata still starts collapsed; current-format records still
omit the legacy title editor. This repairs the title regression in review thread
`discussion_r4299600433` without changing current-format rendering or Files visibility.

Validation: the added legacy visibility assertions failed before the fix. All 151
tests across CurrentOntologyEditor, MarkdownRenderer, ExplicitSaveEditors,
OntologyComparison and OntologyChangesView now pass. Coverage includes editable and
read-only legacy titles, metadata toggling without duplicate titles, title editing
with explicit Save, and unchanged current-format title/heading behavior. Production
TypeScript checking, Prettier and whitespace checks pass. Targeted ESLint has no
errors and six existing warnings in SemanticRecordFrame. No new real-stack browser
run or CI run is claimed for this repair.

### PR #606 seed diagnostics and wording repair (2026-10-19)

Keep captured command stderr as the seed failure diagnostic; when it is missing or
blank, report the exit status without echoing command arguments. The CLI still exits
with status 1 on handled failures. Successful seeding and exact-content verification
are unchanged. Correct only `govern` to `governs` in the ontology links rule.

Validation: three CLI tests cover missing/empty/blank/populated stderr, ordinary
validation errors and successful execution. Those and all six real-Git seed tests
pass; nine opt-in live-stack tests are skipped. Python syntax, the exact one-word
ontology diff and whitespace checks pass. No live-stack or CI run is claimed.

### PR #606 sidebar presentation restoration (2026-10-19)

Restore the pre-rebase Core Ontology presentation over the retained Files-based
navigation: colored book/folder icons, 28px rows, 16px nested indentation, selected
file highlight, and initially collapsed folders. Reuse OntologyChangesButton for
the overview icon, cached count and active-page treatment; pass the active overview
tab through the sidebar. Keep repository/path identity, Files discovery, search,
incomplete-read notices and explain-in-chat navigation. This does not reinstate
projected ontology nodes or change the current sidebar section ordering.

Update the existing expansion regression expectation for the restored collapsed
default. Verification uses the existing g5-lean-seeded pr606-rebased-conflict
workspace in the in-app browser after rebuilding the frontend.

Validation: rebuilt the production frontend (TypeScript and Vite succeeded) and
verified through the in-app browser on localhost:3000 using that seeded workspace.
The overview row has its icon, blue active background and cached count of 1;
root and concepts/data_model open their exact file-based ontology editor URLs.
Selected file highlighting, 28px row height, nested indentation, colored icons,
collapsed folder defaults, mouse expansion and Enter-key collapse were checked.
The overview count and selection remain visible during Refresh. Screenshot:
`/tmp/pr606-sidebar-restored.png`. No test content was edited or saved. Automated
unit tests were not run for this UI-only verification request.

### Ontology statement Markdown cleanup (2026-10-19)

Remove only the redundant initial `- ` from 285 standalone I/D/Q statement bodies
across 18 current ontology files. Preserve all remaining text and whitespace,
anchors, stable IDs, types, paired metadata, navigation lists and genuine internal
lists. No parser or presentation changes. Comparison against an older baseline
will intentionally report these raw-text edits as Modified. Keep this cleanup in a
separate commit from the sidebar repair.

For pre-commit UI verification, stage the ontology-only patch in an isolated
throwaway source checkout and use seed-lean-workspace to pin that source as
`pr606-ontology-dashes` (workspace dde0b99a-1917-4073-af2f-0f023ac25a83).

UI verification passed on localhost:3000 with the fresh g5-lean seed: root and
features/ui statements render with one presentation bullet and no authored dash;
root Reading paths remains a link list; expanded root and UI statement metadata
still contains the original stable IDs, Intent type, human-provided flag and Ben
source links. Reloading the UI file preserves the cleaned text. Screenshot:
`/tmp/pr606-ontology-dashes-removed.png`. This standalone seed has no parent, so
comparison is unavailable; no baseline-comparison verification is claimed. No
application code changed, and no existing workspace content was modified.

### Alignment sidebar CI regression (2026-10-19)

The current-head Cloud Build E2E log showed the Alignment sidebar-order assertion
omitting `ontology` on all three attempts. Restoring Core Ontology intentionally
added it after Activity/optional Guided; the browser expectation still matched main.
Add that entry to the exact expected order, preserving the existing phase/reload
assertions and all production behavior.

Verification: all 27 `TreeNode.sectionOrder.test.tsx` tests passed. Ran the complete
`alignment-shell.spec.ts` Chromium suite against the real local API and browser with
Alignment/Guided enabled. All ten enabled scenarios passed across the full run and
a targeted meeting rerun; authentication also passed, and the Alignment-disabled
scenario was correctly skipped. Logs: `/tmp/pr606-alignment-sidebar-tests.log`,
`/tmp/pr606-alignment-browser-verified.log`, and
`/tmp/pr606-alignment-meeting-ownership.log`. `git diff --check` passed.

Initial local failures exposed occupied worker capacity, a macOS process-worker
port collision, the missing non-live CI provider credential, and seeded repository
ownership incompatible with the root-run dev API's isolated Git configuration.
Used a temporary process worker with fake providers, a temporary port reservation,
the standard non-live credential and fixture ownership correction to complete the
checks. Removed the worker/credential/reservation and restored fixture ownership and
temporary Git trust entries afterward. The pre-existing worker and process-admission
setting were preserved. This verifies browser behavior, not Linux/KVM isolation.

### Route helper and workspace navigation CI repairs (2026-10-19)

Classify `ontology_files.rs` as a non-route helper only in the route-scope guard,
with its authorization boundary documented beside the exemption. Keep the separate
handler-authentication guard active. Runtime authorization still uses the audited
endpoint, workspace read scope and actor-aware Files reader. Update the workspace
browser test inherited from main to require both Files and Core Ontology, activate
Core Ontology and verify its tree is visible. No production behavior changes;
Core Ontology, `.g5/ontology` in Files, Compare and All Changes remain available.

Validation: all five route guards passed with
`cargo test -p g5-api-routes --lib routes::guards::tests`. All 21 selected integration
tests passed with `cargo test -p g5-api --test integration -- api_ontology_
route_auth_static:: authz_membership:: --test-threads=1`, using local PostgreSQL
and isolated test databases. These include missing read scope, cross-organization
denial, repository-qualified ontology Files access, live saves, comparison bounds,
membership removal and static route authorization checks.

The complete `workspace.spec.ts` Chromium suite passed against the existing real
g5-dev API/frontend: five workspace scenarios plus authentication, with isolated
clones and fixture reseeding disabled. All 27 sidebar ordering tests passed.
Rustfmt, focused Prettier and whitespace checks passed. Logs:
`/tmp/pr606-route-guards.log`, `/tmp/pr606-auth-ontology-tests.log`,
`/tmp/pr606-workspace-browser.log`, `/tmp/pr606-sidebar-order-final.log`.
No worker/admission configuration changes were needed. New remote CI results are
not covered by this local verification.

### Render Markdown in statement reading views (2026-10-19)

Use one read-only Markdown body renderer for ontology statements in the editor,
Current/Before comparison rows and the all-changes overview. Render links, code,
escapes, emphasis and genuine lists; retain the outer statement bullet and compact
spacing. Activating editing keeps the exact original Markdown in the existing
plain-text editor. Map rendered text positions back to source offsets so hidden
Markdown syntax does not displace the caret. Links activate navigation rather than
editing, including keyboard activation. Existing repository/path-aware navigation
owns current links; historical links remain disabled because they have no current
navigation identity. Do not change IDs, metadata, saving or Project Docs rendering.

Validation: production TypeScript/Vite build passed. The in-app browser used a
fresh g5-lean seed (`pr606-markdown-reading`, source a51e01d84c37) and isolated child
`pr606-markdown-edit` (f3718ef8-3547-4756-86b7-81e0e4a8ab0c). Verified rendered
relative links, bold text, inline code, escaped underscores and entities; clicking
inline code positioned the plain-text caret inside its original source. Save and
reload preserved the exact Markdown string, including link targets and delimiters.
Current/Before and View all changes render the same saved content; historical links
are disabled. Current links open reference previews from both surfaces; Enter on a
link opens its preview without entering editing, and Open document reaches the
correct repository file. The seeded email consumer statement retains two genuine
lists with six items. UI testing caught and fixed unstable link component identities
that dismissed reference previews on rerender. No Project Docs rendering code was
changed. Existing unit expectations were updated, but automated tests were not run
under the UI-only verification constraint. Screenshot:
`/tmp/pr606-markdown-reading-verified.png`.

Environment setup note: the workspace fork picker reported "Could not switch to
G5 Labs." The isolated seed and child were prepared using the permitted local setup
tools; this run does not claim to fix or verify that unrelated picker issue. Started
local test worker capacity and stopped the dedicated child through the UI after
verification. Existing user workspace content was not edited.

### Markdown image fallback CI repair (2026-10-19)

The frontend lint stage on `1422eefbe` rejected an image-reference anchor whose
href could be undefined. Render a link only for a nonempty sanitized string URL;
otherwise show plain alt text, or `Image` when alt text is empty. Keep the existing
URL transformation and non-navigable historical rendering. No Files/sidebar,
comparison, save or Markdown-source behavior changes.

All 79 tests across OntologyStatementMarkdown, CurrentOntologyEditor,
OntologyComparison and OntologyChangesView passed. Seven image cases cover valid
relative/HTTPS links, empty/rejected URLs, empty alt text and historical references.
The first run exposed two editor tests assuming only one rendered `Second` link;
scope their clicks to the intended statement now that statement Markdown also
renders links, preserving preview, provenance and deletion assertions.
Full `npm run lint` passed with zero errors and 507 warnings; targeted lint,
Prettier and whitespace checks passed. Logs: `/tmp/pr606-markdown-lint.log` and
`/tmp/pr606-markdown-tests.log`. Remote CI remains required before merge.

### Rendered comparison browser repair and formal-review investigation (2026-10-19)

Cloud Build `51059f6f-7fc5-4684-8310-4541d20bc0d8` on `bb5a3e1e7`
failed the comparison browser test because its reading-view locator expected raw
Markdown after this branch introduced rendered Markdown. Select the rendered body
text, keep the exact original Markdown textbox assertion, and scope the metadata
link click to metadata now that the body also renders the same link. Preserve
Save, Undo, comparison, reference-preview and source-persistence assertions.
Commit the parent baseline and stop its runtime before starting the child; durable
Git comparison does not require concurrent live workspaces. Retain the child's
identity before starting it so startup failure cannot bypass child cleanup.
No production UI or API behavior changes.

The same build timed out in the fourth formal-review case, `codex/rejected`,
after the first three provider/verdict cases completed. This scenario and its
production coordinator, fork/workspace services and CI lane configuration are
unchanged from main's `404f8b357` (#642). The logs omit the last review report,
and the PostgreSQL activity snapshot has no blocked sessions. Thus the evidence
does not establish the stalled lifecycle stage or implicate this branch.
The test's 240-second total wait is shorter than one permitted 300-second cleanup
attempt, but the logs do not prove cleanup caused this failure. Preserve the
deadline and add selected lifecycle/report fields to timeout diagnostics so a
recurrence identifies the review, stage, verdict and cleanup state without dumping
the full artifact/execution payload. This is diagnostic coverage, not a claimed
runtime fix. Google Cloud log access succeeded without renewed authentication.

All 79 affected frontend unit tests and all 28 `api_formal_agent_approvals`
integration tests passed. The latter cover authorization, deadlines, cleanup,
provider failure and recovery after lost commit acknowledgments. The modified
Rust E2E binary compiled; its local invocation stopped before the scenario because
the organization environment was missing. Inspection also confirmed the existing
local workers lack the deterministic provider flags required for a faithful
formal-review E2E reproduction; no live provider execution was attempted.
Focused ESLint, Prettier, rustfmt and whitespace checks passed. Logs:
`/tmp/pr606-comparison-unit.log`, `/tmp/pr606-formal-review-integration.log`,
`/tmp/pr606-formal-review-local.log`, `/tmp/pr606-formal-ci-pg.log`.

The complete comparison Chromium suite passed against the real local API, Git,
worker and frontend: two browser scenarios plus authentication (three passes).
This covers stopped-workspace Core/Compare/All Changes/Files and live statement
editing, exact Markdown, Save, Undo, metadata references and comments. Log:
`/tmp/pr606-comparison-browser-final.log`. The isolated fixture was
`pr606-comparison-verification`; initial attempts exposed missing ontology in the
old seed and local process port conflicts. A seed dispatcher failed to stop, and
the root-run local API rejected UID-10001 fixture repositories during Git flush.
After terminating only the identified disposable seed dispatcher, temporarily
matching the two fixture repository roots and the docs `.git` directory to the
API UID allowed the test to complete. Original ownership was restored and all
temporary exact-path Git exceptions were removed. Existing worker processes and
admission settings were not changed. This verifies process-mode browser behavior,
not Linux/KVM isolation. Push and merge are on hold at the user's request; the
formal-review E2E timeout still needs a faithful reproduction or diagnostic CI run.

### Five-commit consolidation and latest-main reconciliation (2026-10-19)

Preserve the four original feature commits and consolidate the eight follow-ups
into one final commit. The original published head `b8c695192` is retained at
`backup/pr606-before-squash-rebase-b8c695192`. The squash had exactly the same tree
as that head. Rebase onto `43416d467`, including main's updates that arrived during
verification. Range comparison confirms the first four feature patches are
unchanged; combined-tree comparison confirms no code changes were lost.

The only textual conflict was `features/chat.g5.md`: keep this branch's removal of
redundant statement bullets while preserving main's exact-revision redelivery
rule and its metadata. Main's native-request producer/API boundary, bounded error
evidence, accepted-answer replay and owner-hold rules also remain intact, with
their stable IDs and paired metadata. Keep main's Base issue-link refusal while
preserving Detach/Unlink for existing links. These commitments are compatible with
repository/path-based ontology editing, Core Ontology, Compare, All Changes and
`.g5/ontology` visibility in Files; no product scope or authority changes were
needed. Update this existing design's verification record in the consolidated
follow-up commit so the branch remains exactly five commits above main.

Verification:

- Final-base Linux Rust 1.97.1: the exact CI command
  `cargo clippy --workspace --all-targets -- -D warnings` passed with the source
  mounted read-only. Native macOS Rust 1.95 reports diagnostics in unchanged
  isolation, host-capacity and document-service code; no lint exceptions or
  unrelated source changes were added. Final rustfmt and whitespace checks passed.
- Final-base API integration: all 177 native-request, task-lifecycle, workspace,
  ontology and authorization tests passed. The first rebase onto `7a317aa5c` also
  passed 230 selected integration tests, including chat controls, Base issue links
  and terminal activity, plus five route guards and 21 ontology/authorization tests.
- Frontend lint and production build passed on the final base. The first rebase
  passed the complete 299-file / 4,692-test suite. The final-base rerun passed 237
  files, including the updated 21-case task-timeline suite, before the local runner
  stopped progressing. The remaining 62 unchanged files could not be rerun: bounded
  launches stalled before test output and were stopped. Do not treat this as a
  completed final-base full-suite run. A subsequent provider-runtime unit-test
  launch also stalled before output and was stopped; Linux Clippy compiled its
  targets, but this run does not claim execution coverage for those unit tests.
- On the first rebased frontend build, real-stack Chromium comparison/workspace
  coverage passed all eight checks, and the targeted Alignment scenario plus auth
  passed. Core Ontology, Files visibility, Compare, All Changes, Save and Undo were
  preserved. These used the existing local API/worker; the final native-request
  backend changes were exercised by the fresh integration binary, not that deployed
  stack. The extra final-base browser rebuild stalled before output and was stopped.
  Temporary fixture-root ownership changes were restored; worker admission and
  existing host processes were unchanged.

Logs: `/tmp/pr606-rebase-linux-clippy.log`,
`/tmp/pr606-rebase-final-integration.log`, `/tmp/pr606-rebase-integration.log`,
`/tmp/pr606-rebase-final-frontend-tests.log`, `/tmp/pr606-rebase-frontend-tests.log`,
`/tmp/pr606-rebase-browser.log`, `/tmp/pr606-rebase-alignment-enabled.log`.
Remote CI remains required; this operation does not merge the PR or resolve the
previous formal-review E2E timeout investigation.
