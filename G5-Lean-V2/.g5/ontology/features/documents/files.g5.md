# Documents: files and access

File identity, discovery, drafts, links, viewers, bounded reads and Save. See [Documents](../documents.g5.md#g5.documents).

## Files and references

<a id="g5.documents.identity"></a>
[D · g5.documents.identity]

A document or file is identified by its workspace, repository and repository-relative path. A title, branch label or projection UUID does not make two files the same document.

<a id="g5.documents.durable-authored-content"></a>
[D · g5.documents.durable-authored-content]

Treat `.g5/ontology` and `.g5/pr_ontology_diff_design`, with their authored descendants, as durable repository content under the shared file-visibility and Git-exclusion policy. Files browsing, staging and explicit Save use that same authority.

- Within either root, the shared path classifier rejects empty, `.`, `..`, `.git` and `.g5` components after normalization.
- The design subtree's Git excludes preserve that boundary for files and directories.
- Plans, mocks and screenshots under the design root are ordinary files; [ontology identity](ontology.g5.md#g5.documents.ontology.current-format) still depends on the ontology root and file format.

<a id="g5.documents.captured-references"></a>
[D · g5.documents.captured-references]

Read a document and resolve its references from the same captured repository views. Unresolved references remain visible as unresolved; mutable database projections never supply missing canonical content or silently select a different branch.

<a id="g5.documents.projection-recovery"></a>
[D · g5.documents.projection-recovery]

Document membership and content come from captured Git and the authorized live overlay, without an ontology or document projection worker, polling repair, or request-triggered indexing. A source change is visible on the next authorized read even when notifications were missed. An unreadable mirror is an error rather than evidence of deletion; reads and notifications never delete document metadata or drafts.

<a id="g5.documents.node-retirement"></a>
[D · g5.documents.node-retirement]

Files and current Markdown are the supported browsing and editing surfaces; Core Ontology is a specialized file-backed view under [current-format navigation](ontology.g5.md#g5.documents.ontology.navigation), sharing repository/path identity and file access with Files. Retire ontology-node APIs, UUID references, node-addressed spec tabs, node navigation and node-based selection rather than rebuilding their cache in requests.

- Preserve document creation, Save, review, explainers and [captured references](#g5.documents.captured-references) under their existing owners.
- After producer retirement deploys, remove the legacy projection tables, node-reference columns and projection-only hooks.
- Before schema contraction, repeat the absence guard: a node-scoped or node-linked comment refuses the migration without converting or deleting discussion.
- Deployment is forward-only; these objects are not kept for rollback.

<a id="g5.documents.generate-inputs"></a>
[D · g5.documents.generate-inputs]

Generate offers committed Project Docs in the workspace's managed folder, excluding dirty files and workflow artifacts. Documents stay selectable without consulting legacy review ledgers, whose presence, content or validity never changes eligibility.

- A missing projection marker establishes no change or completion status.
- Do not maintain a replacement projection or infer generation completion from a repository scan.

<a id="g5.documents.legacy-review-retirement"></a>
[D · g5.documents.legacy-review-retirement]

Retire legacy review-metadata assessments and action selection, including their APIs, implementation scores, Unimpl. filter and Progress dashboard. Preview derives the Human and Risk/Question filters from the document, never from a review ledger — including for empty or uncommitted uploads and legacy `.semlift.md` files.

- Preserve ordinary editing, authored metadata, questions, comments, suggestions, human reviews, approvals and the independent Review Workspace action.
- Historical JSONL fields are ignored without rewriting stored files.
- Links and tabs to the removed dashboard follow the existing [unavailable-surface fallback](../ui.g5.md#g5.workbench.tabs).

<a id="g5.documents.files-view"></a>
[D · g5.documents.files-view]

The Files view browses a repository's files and holds each person's unsaved drafts across the views that open them.

<a id="g5.documents.files-view.drafts"></a>
[D · g5.documents.files-view.drafts]

Unsaved file drafts belong to the person, workspace, repository and path, and survive navigation within the browser tab — Files, Project Docs, Core Ontology and rich/plain text switches — which join the same [file session](editor.g5.md#g5.documents.editor.ownership).

- [Save acknowledgment](../../software_architecture/workspace_vm.g5.md#g5.documents.save.acknowledgment) applies within the captured editing session; completion cannot change a replacement session or recreate a discarded draft.
- Serialize Save across mounted views, keeping submitted bytes as the acknowledged baseline while preserving later typing.
- Native Monaco history survives navigation and mode switches for the tab session, including after Save; explicit discard drops the draft and resets it.
- Changing the signed-in person removes the previous person's drafts, retained models and view selection.
- An authority change keeps recovery text without turning owner edits into reader suggestions.

<a id="g5.documents.source-editing"></a>
[D · g5.documents.source-editing]

Current file openers resolve to one workspace/repository/path tab and FileView session under the [common ownership](editor.g5.md#g5.documents.editor.ownership). Markdown opens rendered and HTML/JSONL in their previews; a line citation opens plain text, and Edit starts in the mode being read — [Rich text](editor.g5.md#g5.documents.preview.rich-editing) from rendered Markdown, plain text otherwise.

- Reopening keeps the selected mode and whether the person was editing; Save and recovery stay reachable from every mode.
- Use the [shared Markdown pipeline](editor.g5.md#g5.documents.markdown.shared) with GFM, math, Mermaid, relative links and source anchors.
- Archive browse opens use current file authority, so an eligible owner can edit and an eligible reader can suggest changes on Project Docs and ontology files too.

<a id="g5.documents.file-links"></a>
[D · g5.documents.file-links]

A workspace file link, `g5://file/<repository-relative path>[?repo=<repository id or name>]`, opens that file in a tab through the [common opener](#g5.documents.source-editing). The workspace's repositories decide its owner, never the path's shape.

- `?repo=` names one repository by id, or by a G5 name only one repository has; otherwise a [Project Docs folder](../workspaces.g5.md#g5.workspace.project-docs) path opens in the docs repository, and any other path opens only when the workspace has exactly one code repository.
- An unknown or ambiguous owner opens nothing and names the candidates; a malformed path or query opens nothing, so a link that names a repository is never read as ownerless.
- Chat, Markdown previews and sandboxed HTML documents accept the same grammar.
- A link grants no access beyond [tenant authority](../../concepts/security.g5.md#g5.security.tenant.rls).

<a id="g5.documents.file-links.line"></a>
[D · g5.documents.file-links.line]

A [file link](#g5.documents.file-links) may end with a line fragment `#L<n>` or `#L<n>-L<m>` after any `?repo=`, opening the file at line `n` like a [line citation](#g5.documents.source-editing), with the line grammar relative links already use.

- Any other fragment opens the file without a line, and never changes which repository or file opens.
- A `?` after the `#` is a misplaced query that makes the link malformed, so a misplaced `?repo=` is never ignored.
- The ontology editor still resolves fragments as [headings](ontology.g5.md#g5.documents.ontology.links), where ontology files use relative links.

<a id="g5.documents.reader-suggestions"></a>
[D · g5.documents.reader-suggestions]

Eligible non-owner readers may propose changes to existing text files without a fork or attached compute. A push records additive suggestions for the source owner and changes no source file or branch.

- The proposer may withdraw their earlier open suggestions rather than overwrite them.
- A stale submitted base, detected against the authorized read used for comparison, shows the difference and requires explicit confirmation before the proposal publishes.
- Source content policy and [viewer collaboration](../organizations.g5.md#g5.organization.viewer-collaboration-question) still apply; this does not change [committed reader source selection](../workspaces.g5.md#g5.workspace.durable-read).

## File viewers

<a id="g5.documents.viewers"></a>
[D · g5.documents.viewers]

Raster images and PDFs opened from Files, Project Docs or a file link show in a read-only viewer in the same file tab, under the same identity, [bounded reads](#g5.documents.read.bounds) and live-versus-committed selection as text files. A viewer has no content authority or write path, and offers no editing, suggestions or comments.

- The extension alone classes a file: `.pdf` opens the [PDF viewer](#g5.documents.viewers.pdf), a supported raster extension opens the [image viewer](#g5.documents.viewers.images) whatever its bytes hold, SVG keeps plain text editing with its image as the rendered [mode](editor.g5.md#g5.documents.editor.modes), and every other binary file keeps the Download card.
- The 32-MiB read ceiling still applies: a larger file is refused before its bytes load, and the tab says so rather than offering a download it cannot make.
- A viewer decodes only the bytes it was given, and on a file, version or tab change it cancels and releases obsolete loads, renders, workers and object URLs, so a stale result never replaces a newer file.
- It keeps Download, under the file's own type, whenever admitted bytes do not decode.

<a id="g5.documents.viewers.images"></a>
[D · g5.documents.viewers.images]

The image viewer shows PNG, APNG, JPEG, GIF, WebP, AVIF, BMP and ICO through the browser's own decoder, keeping the transparency, animation and EXIF orientation the browser supports. Images embedded in rendered Markdown use the same decoder.

- It offers Fit to panel (never above actual size), Actual size, Zoom between 5% and 800%, scrolling, a checkerboard behind transparency, the pixel and byte sizes, and Download.
- SVG renders only in an image context, where its scripts, external references and interaction cannot run; it is never injected into G5's document or opened as an active document.
- An image whose header declares more than 64 megapixels is not decoded and shows its dimensions with Download; PNG, JPEG, GIF, BMP and WebP headers are probed, and other formats rely on the browser's own limits.
- HEIC/HEIF, TIFF, PSD, camera RAW and JPEG XL stay outside the set and keep the Download card; drag, pinch and trackpad gestures are not provided until asked for.

<a id="g5.documents.viewers.pdf"></a>
[D · g5.documents.viewers.pdf]

The PDF viewer renders documents in the browser with Mozilla PDF.js (`pdfjs-dist`, Apache-2.0), pinned to a maintained release with the 2026 scripting fix and served from G5's own hashed assets. It runs no document-viewing service, scripting, XFA or annotation layer, so links and forms inside a document are inert.

- It offers page navigation, fit to width (the opening zoom), zoom presets, rotation, text selection, search over the document's own text, and Download.
- A search match is a text run containing the term; scanned pages without text find nothing, and OCR is out of scope.
- Loading shows progress and can be cancelled; a password-protected document asks with retry and Cancel; a corrupt, unsupported or cancelled document says why and keeps Download.
- Only pages near the viewport render, each to a canvas of at most 16 megapixels (device pixel ratio at most 2) with its text layer; pages that scroll away release their canvas, and a replaced or closed document destroys its loading task, worker and pages.
- Documented support is Chrome and Edge 125+, Firefox ESR+ and Safari 18+ with reported limits; G5 verifies Chromium automatically, while Firefox, Safari and Edge on Windows await a person's validation.

## Bounded repository reads

<a id="g5.documents.read"></a>
[D · g5.documents.read]

File, document and service readers share one bounded way to read committed repository content.

<a id="g5.documents.read.bounds"></a>
[D · g5.documents.read.bounds]

Bound committed reads before allocating content; file, document and service readers share this admission. One Files-tree entry and byte allowance is shared across requested roots, with dirty overlays restricted to those roots through bounded object-header observations.

- Browser listings disclose truncation; a service enumeration requiring completeness fails with `TREE_TOO_LARGE` rather than return partial results.
- Reject an oversized file from its object header before loading it, using the 32-MiB ceiling or a narrower caller limit.

## Explicit file Save

<a id="g5.documents.selected-save"></a>
[D · g5.documents.selected-save]

An owner's explicit file Save applies [VM preconditions](../../software_architecture/workspace_vm.g5.md#g5.documents.save) and [file mutation rules](../../software_architecture/workspace_vm.g5.md#g5.runtime.files.mutation), then checkpoints only the selected file's submitted content and preserves unrelated staged work. Ordinary file editing and question answers share this operation.

- A Save whose bytes landed but whose checkpoint or push failed returns a distinct "applied, publication failed" outcome with the applied version, never a plain failure, so nobody redoes a Save that succeeded.
- An equal-content retry may publish without rewriting; a changed file or source head must not silently replace the selected content.

## Metadata

### g5.documents.identity

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: [workspace reconstruction](../../../uplift/ben-ontology/investigations/workspace.md), `doc_service` lines 4811–5044/5132–5298 and `doc_reference_service` lines 1–335. Identity layout is inspected structure, not a model result.

### g5.documents.durable-authored-content

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Extends the existing [ontology navigation](ontology.g5.md#g5.documents.ontology.navigation) policy with the design subtree requested in [issue #794](https://github.com/G5Labs/G5-Lean/issues/794).
- Design, scope and verification: [durable design artifacts](../../../pr_ontology_diff_design/issue-794/ontology-plan.md). The shared owner is `g5_contracts::g5_paths`; existing file and Git consumers retain their authorization and source-selection rules.

### g5.documents.captured-references

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source and checks: [workspace reconstruction](../../../uplift/ben-ontology/investigations/workspace.md).
- DocumentReference checked that committed document/reference reads use captured repository versions rather than a lagging projection. Parsing, ambiguous-name resolution and live filesystem snapshot atomicity are not proved.

### g5.documents.projection-recovery

- human-verified
- Expands: [Documents](../documents.g5.md#g5.documents).
- Retirement source: [issue #620, PR B](https://github.com/G5Labs/G5-Lean/issues/620) and the [continuing design](../../../pr_ontology_diff_design/qa-workspace-lock-contention/ontology-plan.md). Implemented in the captured document reader; `git_refs_authoritative` exercises add/modify/rename/delete and missed notifications with empty caches, and verifies that an unavailable mirror returns an error without deleting metadata. `projection_retirement` exercises the actual maintenance lifecycle, hints, timers and restart without repository scans or projection writes.
- The earlier [workspace reconstruction](../../../uplift/ben-ontology/investigations/workspace.md) and ProjectionConvergence evidence concern the retired polling mechanism, not this read-driven contract. Preserve the refusal to infer deletion from an unavailable mirror; no new model proof is claimed.

- PR448 integration retains historical uplift/downlift timestamps and commit markers for authorized UUID document/view reads while file-derived fields still use the authorized source. The real canonical refresh regression covers both routes; all 13 `git_refs_authoritative` cases pass after repair. [Integration evidence](../../../../docs/review-flow.md#october-19-latest-main-integration).

### g5.documents.node-retirement

- human-verified
- Expands: [Documents](../documents.g5.md#g5.documents).
- [File-backed Core Ontology amendment, 2026-10-19](../../../chat_intent/saehan-499-ontology-editor/file-backed-integration.md) preserves retirement while permitting the specialized file view.
- Sources: [issue #620](https://github.com/G5Labs/G5-Lean/issues/620), merged [PR B](https://github.com/G5Labs/G5-Lean/pull/630), and the owner's [forward-only deployment decision](../../../chat_intent/qa-workspace-lock-contention/forward-only-retirement.md).
- Runtime retirement is implemented across backend contracts/routes and frontend navigation, editor, chat, comments and explainers. PR B's HTTP and CI regressions passed. PR C's restricted-role populated and fresh migrations, refusal guard, retained-row comparisons and final-schema consumer regressions passed locally under the [continuing design](../../../pr_ontology_diff_design/qa-workspace-lock-contention/ontology-plan.md). Local evidence does not establish deployed QA workspace journeys. Current authorization and provider synchronization are unchanged.

### g5.documents.generate-inputs

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Decision: [PR B design](../../../pr_ontology_diff_design/qa-workspace-lock-contention/ontology-plan.md), based on Generate's existing committed-Project-Docs selection and the issue's missing-marker regression. Implemented in `DocService::list_pending_downlift_docs`; the HTTP regression in `git_refs_authoritative` verifies selectable committed input with no marker rows and no invented changed reason.
- Review-ledger dependency removed by [issue #850's removal plan](https://github.com/G5Labs/G5-Lean/issues/850#issuecomment-6434584909); [design](../../../pr_ontology_diff_design/retire-frontend-ground-trace-code/ontology-plan.md).

### g5.documents.legacy-review-retirement

- human-verified
- Expands: [Documents](../documents.g5.md#g5.documents).
- The owner requested work on [issue #850](https://github.com/G5Labs/G5-Lean/issues/850), focusing on its [Proposed removal plan](https://github.com/G5Labs/G5-Lean/issues/850#issuecomment-6434584909). [Implementation design](../../../pr_ontology_diff_design/retire-frontend-ground-trace-code/ontology-plan.md).

### g5.documents.files-view

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.files-view` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Drafts](#g5.documents.files-view.drafts).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.files-view.drafts

- agent
- Expands: [Files view](#g5.documents.files-view).
- Source: [workspace reconstruction](../../../uplift/ben-ontology/investigations/workspace.md); draft keys and person-change clearing in `fileDrafts` lines 1–185 were inspected.
- SaveAcknowledgment checked that completion records submitted bytes without overwriting later edits or a replacement editing session. It covers one pending save, not all overlapping saves or browser-restart persistence.
- Scope extended by the [unification design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md). Current implementation tests cover shared Save coordination and retained models/view selection; the earlier model is not a proof of that implementation.

### g5.documents.source-editing

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Opening defaults and implementation choices from [issue #793](https://github.com/G5Labs/G5-Lean/issues/793) and the [shared editor design](../../../pr_ontology_diff_design/unify-file-editing-and-render/ontology-plan.md). Common ownership, source engine and preview semantics now link to their dedicated blocks after reconciliation with Base's parallel design.
- Edit opening Rich text from a Markdown Preview follows the owner's [rich-editing decision of 2026-10-29](../../../chat_intent/rich-text-editing/request.md#human-decision--rich-editing-in-preview--2026-10-29); it replaces the 2026-10-27 clause that kept Preview selected for a block edit. Starting in the view being read, so Source stays Source, is an agent choice in the [plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#rich-editing-in-preview--2026-10-29).
- The owner's [one-surface decision of 2026-10-30](../../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30) makes the mode being read the start of editing and retains whether the person was editing ([plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#one-surface-with-a-richplain-text-switch--2026-10-30)).

### g5.documents.file-links

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Records the existing grammar and ownership rule (center-panel D73, issue #1970 in the earlier ontology), now relied on by [chat file citations](../chat.g5.md#g5.chat.file-citations). [Design](../../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md).
- Implemented: `parseG5FileHref`, `sanitizeWorkspaceFilePath`, `sanitizeWorkspaceRepoRef` and the in-frame copy `SANDBOXED_FILE_NAV_JS` (`frontend/src/components/shared/sandboxedHtml.ts`); `resolveDeepLinkRepoId` (`frontend/src/lib/workspaceBranches.ts`); the document delegate and `handleOpenDeepLinkFile` in `WorkspacePage.tsx`; re-validation in `SandboxedHtmlFrame.tsx`. Tests: `sandboxedFileNav.test.ts`, `MarkdownPreview.links.test.tsx`, `workspaceBranches.test.ts`.
- Owner check of 2026-10-27 (question `verified-click-behaviour` in the Project Docs set `docs/dc2d7087-7895-46c6-a5ca-f20e910fba19/questions/agent-file-links.g5q.json`, docs repository): a Project Docs link and an unqualified code link in a one-code-repository workspace open from chat.
- Evidence, 2026-10-27 ([verification](../../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md#implementation-and-verification--2026-10-27)): `e2e/flows/agent-file-links.spec.ts` clicks Project Docs and code links in Claude and Codex replies and a `?repo=<repository id>` link and an unknown-repository link in a sandboxed explainer in Chromium against the process-isolation dev stack; `WorkspacePageFileLinks.test.tsx` covers a two-code-repository workspace (qualified opens the named repository, unqualified is refused with the candidates). A live click in a real two-code-repository workspace was not run.
- Only the workspace page opens these links; Home's side panel does not ([personal exception](../personal_workspace.g5.md#g5.personal.file-citations)). The [Diff Graph](../diff_graph.g5.md#g5.diff-graph.relationships) classifies only qualified links and also accepts a repository slug; it opens no files.

### g5.documents.file-links.line

- human-verified
- Expands: [File links](#g5.documents.file-links).
- Owner answer "Links must open at the line" (question `line-references`, 2026-10-27, same set), which left the format to System Engineering. The `#L` grammar, shared with `resolveRepoLink` (`frontend/src/components/files/repoLinks.ts`), and opening other fragments without a line are agent choices. Refusing a `?` after the `#` was found in review: splitting the fragment first would otherwise turn `…#L12?repo=api` into an unqualified link.
- Implemented: `lineFromFragment` (`repoLinks.ts`) owns the line grammar; `parseG5FileHref` and the in-frame `SANDBOXED_FILE_NAV_JS` return or post `line`; `SandboxedHtmlFrame` drops a proposal whose `line` is not a positive safe integer; the `WorkspacePage` delegate passes the line to the citation pathway, which opens Source and reveals the line (the editor clamps a line past the end). The ontology editor's heading resolution (`CurrentOntologyReference.tsx`) is unchanged.
- Evidence, 2026-10-27 ([verification](../../../pr_ontology_diff_design/issue-875-agents-cite-workspace-files/ontology-plan.md#implementation-and-verification--2026-10-27)): `sandboxedFileNav.test.ts` (both parsers: line, range, qualifier with line, other fragments, refused misplaced query), `SandboxedHtmlFrame.test.tsx` (forwarded and forged lines), `WorkspacePageFileLinks.test.tsx` (line opens Source, heading opens Preview; fails if the delegate drops the line), and the browser flows above, which check that Monaco shows line 150 and not the top of the file.

### g5.documents.reader-suggestions

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: human [source-editor request](../../../chat_intent/files-source-editor/request.md), accepted reader scope and `workspace_reviews` lines 960–1177.
- [ReaderSuggestions](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked additive suggestions without source writes, stale-base confirmation and withdrawal while compute is stopped. Comparison precedes commit, so latest-at-publication freshness is not guaranteed; the committed-read gap still applies.
- Amendment: [supplied proposal](../../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 8 (DC-Q1, OR-cell-16).

### g5.documents.viewers

- human-provided, agent
- Source: the owner's [issue #982](https://github.com/G5Labs/G5-Lean/issues/982) and its two recommendation comments of 2026-10-31 (image and PDF), authorized in [chat](../../../chat_intent/support-viewing-image-and-pdf-via-files-tab/request.md). Classing by extension, the over-ceiling message and the absence of comments on viewed files are agent choices in the [plan](../../../pr_ontology_diff_design/support-viewing-image-and-pdf-via-files-tab/ontology-plan.md).
- Expands: [editor modes](editor.g5.md#g5.documents.editor.modes); relies on [bounded reads](#g5.documents.read.bounds).

### g5.documents.viewers.images

- human-provided, agent
- Source: the owner's image recommendation on [issue #982](https://github.com/G5Labs/G5-Lean/issues/982) (native decoding in `<img>`, the control set, SVG in an image context, the format set, decoded-size bounds). The 100% fit cap, the 64-megapixel bound, the probed formats and starting without a gesture library are agent choices in the [plan](../../../pr_ontology_diff_design/support-viewing-image-and-pdf-via-files-tab/ontology-plan.md#image-viewer).
- Expands: [viewers](#g5.documents.viewers).

### g5.documents.viewers.pdf

- human-provided, agent
- Source: the owner's PDF recommendation on [issue #982](https://github.com/G5Labs/G5-Lean/issues/982) (PDF.js through `pdfjs-dist`, the control set, operating limits, licensing). The compatibility build, the per-document worker, leaving out the annotation layer, the search semantics and the canvas cap are agent choices in the [plan](../../../pr_ontology_diff_design/support-viewing-image-and-pdf-via-files-tab/ontology-plan.md#pdf-viewer); the browser matrix is upstream's documented support, not a G5 guarantee until a person validates Firefox, Safari and Edge on Windows.
- Expands: [viewers](#g5.documents.viewers).

### g5.documents.read

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.read` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Bounds](#g5.documents.read.bounds).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.read.bounds

- agent
- Expands: [Read](#g5.documents.read).
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/documents.g5.md).
- PR #496 adds multi-root and dirty-overlay regressions. This supplements [captured references](#g5.documents.captured-references); it does not change which committed cut a reader may access.
- PR448 review comparison sides inspect object headers before loading; complete bounded tree enumeration is required for all-file/membership summaries. Six real review-flow API regressions include the32MiB+1 refusal, binary and missing-path behavior. Current-head Cloud qualification remains recorded in [the design](../../../../docs/review-flow.md).

### g5.documents.selected-save

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: [workspace investigation](../../../uplift/ben-ontology/investigations/workspace.md), `workspace_file_save` and native checkpoint.
- SelectedSave found that native A→B→A edits can stage B while the hash check accepts A; a single-capture comparator preserves submitted bytes, selected scope and unrelated staged work. [WORKSPACE-SAVE-BLOB](../../../../docs/code_gaps.md) remains; current-owner runtime is reacquired between operations, while Git filtering and filesystem durability are abstracted.
- Amendment: [supplied proposal](../../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 12 (DC-Q2).
