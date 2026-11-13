# FILES source editor — read-first view and reader file suggestions

## Ontology changes and governance

**Comparison base:** `origin/main` at `f19cc664`, branch `zcl/files-source-editor`. The [chat intent](../../chat_intent/files-source-editor/request.md) holds the human intent and the agent proposals the user accepted.

**Ontology changes:**
- `features/documents.g5.md` adds:
  - the story `g5.story.read-file-first`;
  - `g5.documents.editor.assets`;
  - `g5.documents.files-view` and its children `.preview`, `.source-only`, `.text-only`, `.drafts`, `.owner-edit`, `.compare`, `.refresh` and `.annotations`.

  It also corrects the stale observed-VM gap in the `g5.documents.editor.source` metadata.
- `features/governance.g5.md` adds:
  - the story `g5.story.suggest-file-change`;
  - `g5.governance.file-suggestions` with `.additive`, `.stale`, `.record` and `.base`.

  It also names file suggestions as a second producer in `g5.governance.review-suggestions`.
- `concepts/core_user_stories.g5.md` links the two new stories.

Every new block's metadata started as not implemented. After implementation, the [implementation review](ontology-review.md) recorded locations, evidence and gaps in those footers and in `g5.documents.read.labels`, `g5.documents.save.disk-change`, `g5.documents.save.acknowledgment`, `g5.documents.save.reload` and `g5.governance.review-suggestions`; no block text changed. The scoped diff, metadata included, follows the [review summary](#review-summary).

### Relevant and governing ontology

| Block | Why relevant | Implementation must enforce or consider |
|---|---|---|
| [Serve nonowners from durable state](../../ontology/concepts/security.g5.md#serve-nonowners-from-durable-state) | Readers suggest against what they read | The server reads the base exactly as the reader's view does: no live fallback. |
| [Separate membership from write permission](../../ontology/concepts/security.g5.md#separate-membership-from-write-permission) | Readers create suggestions | A push writes platform records only, never workspace content. |
| [Let readers reply in threads](../../ontology/features/documents.g5.md#let-readers-reply-in-threads) | The precedent for reader-written durable records | The same writability rule applies: no Base, read-only or freezing source. |
| [Show the selected durable source truthfully](../../ontology/features/documents.g5.md#show-the-selected-durable-source-truthfully), [labels](../../ontology/features/documents.g5.md#label-the-source-being-shown) | The read view and the suggestion base | Code view and Preview both keep the source badge. |
| [Save binds bytes and the attachment](../../ontology/features/documents.g5.md#save-binds-bytes-and-the-attachment-they-came-from), [acknowledge only submitted edits](../../ontology/features/documents.g5.md#acknowledge-only-the-submitted-edits), [explicit Save checkpoint](../../ontology/features/documents.g5.md#publish-an-explicit-editor-save), [reload before editing](../../ontology/features/documents.g5.md#reload-before-editing-a-new-attachment) | Owner Save is unchanged | A draft that outlives its editor keeps its baseline hash and VM, and is acknowledged only for submitted bytes. "Start workspace to edit" reloads live bytes before enabling Edit. |
| [Expose concurrent disk changes](../../ontology/features/documents.g5.md#expose-concurrent-disk-changes) | Agent writes to an open file | A dirty FILES editor shows changed-on-disk with Compare and Reload; a clean one reloads. |
| [Bound decoded file payloads](../../ontology/features/documents.g5.md#bound-decoded-file-payloads) | Input size | The 2 MB editor limit sits inside the 32 MiB bound; the suggestion route bounds both texts. |
| [Retain block-style spec editing](../../ontology/features/documents.g5.md#retain-block-style-spec-editing), [suggestion projection](../../ontology/features/documents.g5.md#limit-suggestion-rendering-to-the-speceditor-entry-point) | SpecEditor must not change | All new presentation code sits behind `case "file"`; SpecEditor suites run unedited. |
| [Review each document edit independently](../../ontology/features/documents.g5.md#review-each-document-edit-independently), [independent decisions](../../ontology/features/documents.g5.md#retain-independent-decisions-within-a-review-publication), [placement](../../ontology/features/documents.g5.md#surface-suggestion-placement-uncertainty) | File suggestions are ordinary suggestions | Reuse `insert_generated_suggestions`, `discrete_edits` and the existing placement; add no new decision path. |
| [A review returns suggestions](../../ontology/features/governance.g5.md#a-review-returns-suggestions-to-the-source-owner), [display](../../ontology/features/governance.g5.md#show-a-suggestion-the-same-way-everywhere), [review threads](../../ontology/features/governance.g5.md#let-every-reader-join-a-review-thread), [notices](../../ontology/features/governance.g5.md#notify-the-other-party-once) | Decisions, threads and notices | Accept, Reject and Withdraw stay the same; the owner is notified once per push. |
| [Publish accepted review files](../../ontology/features/governance.g5.md#publish-accepted-review-files) | Owner acceptance | Unchanged; Accept still needs the owner's running VM. |
| [A file's comments follow the file](../../ontology/features/documents.g5.md#a-files-comments-follow-the-file), [offer only permitted comment actions](../../ontology/features/documents.g5.md#offer-only-permitted-comment-actions) | Threads in Preview and read-only Monaco | Preview lists the same threads. Read-only Monaco keeps every comment control the viewer offered. |
| [Justify each new mechanism](../../ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | Minimal design | No migration, flag, rollout or compatibility layer: G5-Lean has no external users, and a QA outage or manual step is acceptable. |
| [Test real boundaries](../../ontology/concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence) | Evidence | Run real API, browser and end-to-end tests, and report anything not executed. |

## Requested outcome

FILES behaves like GitHub's file page. A file opens for reading: Markdown in a rendered Preview, other text in a read-only Monaco Code view. Edit turns the same view editable. The workspace owner saves as today: each Save commits and pushes that one file. Anyone else who can read the workspace clicks Suggest changes, and their Save button becomes Push review. Push review turns the edit into review suggestions for the owner, with no fork and no running VM.

The research found several flaws that this change fixes along the way:
- binary files opening as editable base64;
- unsaved edits lost on view switches;
- a dirty editor silently deferring disk changes;
- readers' views never refreshing;
- Monaco loading from a public CDN.

SpecEditor and PROJECT DOCS do not change. The [chat intent](../../chat_intent/files-source-editor/request.md) lists every accepted decision.

## Design

```text
FILES tree ─▶ WorkspacePage `case "file"` ─▶ FileView (FILES only)
   ├─ not text (UTF-8 without NUL) or over 2 MB ─▶ card: name, size, Download
   ├─ Preview (Markdown) ─▶ MarkdownPreview: react-markdown + GFM, math, Mermaid, sanitized HTML
   │                         └─ beside it: the file's threads and pending suggestions (SideThreads)
   ├─ HTML ─▶ existing rendered/source shell; FILES keeps the draft across modes
   └─ Code / Edit ─▶ CodeEditor with bundled Monaco: read-only ⇄ editable in place
          draft ⇄ fileDrafts store (browser tab; key: person/workspace/repo/path)
          beforeunload while any draft exists; workspace page mirrors its drafts into dirtyEditors

owner,  VM running ── Edit ─ Save ─▶ PUT /workspaces/:id/files/:path {hash, vmId}
                                     ─▶ commit "Save {path}" + push                         (unchanged)
owner,  VM stopped ── "Start workspace to edit" ─▶ Start ─▶ live reload ─▶ Edit
reader, any VM ────── Suggest changes ─ Push review (optional note)
   ─▶ POST /workspaces/:id/review/file-suggestions {repoId, path, baseContent, content, note, allowStaleBase}
      API: read access · not the owner · source accepts thread writes · all texts UTF-8 without NUL
           ─▶ read the current durable file as this reader sees it (must be text)
           ─▶ blobOid(baseContent) ≠ current hash and not allowStaleBase ─▶ 409 FILE_VERSION_CONFLICT
           ─▶ g5-scm text diff (same limits and CR handling as rounds) ─▶ hunk rows
           ─▶ new annotation publication (no review workspace) + insert_generated_suggestions
           ─▶ notify the owner once
owner reviews cards: Accept (live VM) / Reject; reader Withdraw                              (unchanged)
```

**FileView** is a new FILES-only component that owns the read-first presentation:
- It picks the mode from the file's text eligibility, size and extension, and from any line target.
- It decides who may edit or suggest from the existing page flags: `isWorkspaceOwner`, the running attachment, and `workspaceAcceptsThreadWrites`.
- It keeps one Monaco instance across Code and Edit.

**Text eligibility** means valid UTF-8 with no NUL byte, at most 2 MB. It is a single frontend check applied to live and durable reads alike, because the read routes label NUL-containing UTF-8 as text. The suggestion route applies the same rule to the current file, the starting text and the proposed text.

**CodeEditor** gains opt-in props that keep today's behaviour for every other caller (ontology code nodes, PROJECT DOCS JSONL, job output):
- read-only Monaco;
- a draft held outside the component, which then skips its own `dirtyEditors` registration;
- Compare in the Save-conflict dialog and a changed-on-disk notice;
- an extra toolbar slot.

Read-only Monaco keeps every comment control the highlight.js viewer offered. The per-line add control lives only in the viewer branch today, so the Monaco branch gets an equivalent glyph-margin control. Owner thread creation and reader replies are verified in a real browser.

**Drafts** live in a module store for the browser tab, keyed by person, workspace, repository and path, with entries for other people dropped on sign-in change. Going to another G5 page or workspace therefore keeps them, and returning restores them in Edit mode. The app's `BrowserRouter` has no navigation blocker, and the scoped composer store resets when the workspace changes, so neither can carry drafts across navigation. Only closing or reloading the tab can lose a draft, so `beforeunload` prompts while any exist. The workspace page mirrors its own drafts into `dirtyEditors` by path, so a review copy's Discard still drops them, including every same-path draft across repositories.

**MarkdownPreview** is new. The shared `MarkdownRenderer` stays untouched, because chat and SpecEditor surfaces depend on it.

**Monaco** 0.55.1, the version the loader fetches today, is bundled through `loader.config({ monaco })` with Vite worker imports. It loads only when a Code view opens.

**Reader refresh** adds the FILES file query to the existing branch, flush and authoritative-cut invalidation. Those events are published to the workspace channel, while `workspace.file_changed` goes only to the owner.

**The server route** lives beside Push review and reuses its parts:
- `create_publication` with kind `annotation` and no review workspace, so pushes never supersede one another or any round;
- `insert_generated_suggestions` for hunk rows, per-edit identities, notes and threads;
- the owner notification.

The route sets its own request-body limit: two 2 MB texts at worst-case JSON escaping (6 bytes per control character) plus a note. It still bounds each decoded text at 2 MB.

A new `g5-scm` helper diffs two buffers with libgit2's `Patch::from_buffers`, under the same 100 KiB / 3,000-line limits and carriage-return handling as review rounds. The per-file hunk-to-suggestion conversion is shared with round generation rather than copied. A push whose only change is the final newline proposes nothing; it is refused, and the draft is kept.

**Starting text.** The client sends back the text it started from, and the server compares that text's Git blob hash with the reader's current durable file. A mismatch returns `FILE_VERSION_CONFLICT`, and the UI offers Compare or Push anyway. The server never reads a commit or blob the client names, because repository storage can be shared between workspaces, so it reads only what this reader may currently read. A deliberately stale start is the reader's own unverified text. Its suggestions may still place where their text matches, but like every suggestion they change nothing until the owner accepts them.

**Implementation notes.** These decisions were made while building and explain the final behaviour:
- The route checks the texts and computes the diff before judging staleness, so a push that could never be recorded is refused outright instead of being offered Push anyway.
- Refusals carry `FILE_SUGGESTION_REFUSED` with the field and a readable message, which the error envelope repeats. An owner's push gets 409.
- The buffer diff treats both texts as text whatever libgit2's binary guess says; the route's own text rule decides.
- An obsolete suggestion from a push without a review workspace tells the reader to suggest the change again from FILES.
- CodeEditor releases a draft that ended (Discard, a push, a drop elsewhere) and shows the current file, so the mounted read view keeps nothing stale. The changed-on-disk notice gives the owner Save my version, as `g5.documents.files-view.compare` asks.
- Drafts are keyed by person, not principal, so an org switch keeps them; `RequireAuth` retains the signed-in person's drafts once auth has loaded.
- A Push review in flight marks its draft, so every view of the draft takes no typing until the push settles; completion drops only the version pushed. Push requests carry the rendered principal.
- A Save completion belongs to its editing session; once the draft is discarded, dropped, replaced or left, the completion only acknowledges the submitted bytes in the store.
- A Save in flight marks its draft, so every view keeps it whatever its text; settling a Save moves only the baseline in the store, and a clean view keeps its buffer until the Save settles.
- When a view's Save settles, a file version that arrived meanwhile is judged against the draft's `lastSave` record (the version replaced and the version written). A version a successful Save superseded is dropped, with any notice about it. Any other version that differs from the baseline is a disk change: a clean buffer takes it, an edited one gets the changed-on-disk notice. Adopting a stored baseline re-renders the editor, so the dirty state and Save follow it. A view reopened during a Save waits for it before raising a notice.
- `useSaveWorkspaceFile` writes the saved bytes into the file cache only while the cache holds the version the Save replaced, or the saved version. Otherwise it reads the file again, so a newer version read meanwhile is never overwritten by an older answer.
- `dirtyEditors` keeps one registration per owner and path, so one editor turning clean never unlists the page's FILES drafts on the same path.
- Preview shows the source badge in the FileView header, sharing `SourceBadge` with the editor toolbar.

## Acceptance outcomes

1. **Default modes.** From FILES, a Markdown file opens in a rendered Preview showing GitHub-flavored content, and other text files open in read-only Monaco. A link to a line opens Code view at that line. HTML keeps its current preview.
2. **Owner editing.** Edit makes the same view editable. Markdown edit mode has Edit and Preview tabs, and Preview shows the unsaved text. Save commits and pushes as today. A stopped owner sees "Start workspace to edit", and editing begins only on live bytes.
3. **Unsaved edits.** Switching between Preview, Code and Edit, HTML modes, tabs, panels or G5 pages keeps unsaved text, which is acknowledged only for submitted bytes. Closing or reloading the tab with unsaved text asks first.
4. **Reader suggestions.** A non-owner reader can Suggest changes and Push review with an optional note, whether the owner's VM is running or stopped. The owner sees the change as suggestion cards in Code view and listed beside Preview, and accepts it with a running VM. Repeated pushes add suggestions, and the reader can withdraw them.
5. **Refusals.** Each of these is refused with a clear message, and a refused push keeps the draft:
   - the owner pushing to their own workspace;
   - Base, read-only and freezing sources;
   - non-readers;
   - files that are not text;
   - no change, or only a final-newline change;
   - diffs over the limits.

   A small edit to a file just under 2 MB is accepted.
6. **Stale start.** A push against a file that changed warns and offers Compare. Push anyway records suggestions that place where their text still matches.
7. **Conflicts.** An owner Save conflict and changed-on-disk both offer Compare, Reload and Save my version, and Compare keeps the draft.
8. **Non-text files.** A file that is not UTF-8 without NUL, or a text file over 2 MB, shows a card with Download and no editor. This holds for live and durable reads.
9. **Reader refresh.** A reader's clean view refreshes when the source's branch, flush or cut advances.
10. **Bundled Monaco.** Monaco loads with requests to jsDelivr blocked.
11. **SpecEditor unchanged.** SpecEditor and PROJECT DOCS behave as before, and their test suites pass without edits to their assertions.

**Out of scope:**
- editing in the rendered Preview;
- suggesting new files, renames or deletions;
- previews for other formats;
- a commit dialog;
- rendering changes to PROJECT DOCS views;
- feature flags, staged rollout or compatibility shims.

## Test plan

**Rust API** (`backend/g5-api/tests/api_workspace_reviews.rs`):
- a reader's push: hunks, edits, the thread opened by the note, the owner notice, kind `annotation` and no review workspace;
- refusals: the owner; Base, read-only and freezing sources; a non-reader; NUL or non-UTF-8 files and texts; no change and final-newline-only; diffs over the limits;
- a small edit to a file just under 2 MB is accepted under the route's body limit;
- a stale 409, then Push anyway with a pure insertion and an ambiguous block, and the owner Accept outcome for each;
- a flush-snapshot base read while the owner's VM is stopped;
- two pushes plus a later round, leaving the pushes open;
- CRLF kept;
- owner Accept through the runtime peer, and reader Withdraw.

**Rust unit:** the `g5-scm` buffer diff (carriage returns, pure insertion, limits) and the shared conversion.

**Frontend unit (vitest):**
- FileView:
  - modes, the line target, and in-place Edit;
  - the non-text card for base64 content and for NUL-containing live and durable reads, and the too-large card;
  - Start to edit;
  - the suggest, push and stale flow, with a refusal keeping the draft;
  - draft survival across remounts and workspace navigation;
  - editing, going to Home, then reloading still prompts;
  - a change of person clears drafts, and a late Save or Push completion cannot recreate a cleared draft;
  - edits made while Save or Push is pending staying dirty;
  - `beforeunload`;
  - the `dirtyEditors` mirror.
- MarkdownPreview: GFM features, alerts, math, Mermaid, relative links and images, and sanitization of scripts, event handlers and `javascript:` links.
- CodeEditor opt-ins: read-only Monaco comment controls, and Compare keeping the draft.
- The reader-refresh invalidation, and the Reviews panel label for non-round publications.

**Playwright** (new `frontend/e2e/flows/files-source-editor.spec.ts`):
- the owner path: Preview, Code, Edit, unsaved text across a tab switch and a trip to Home, Save and commit, the binary card, and owner thread creation in read-only Monaco;
- the reader path with the owner's VM stopped: a reply in read-only Monaco, Suggest changes, Push review, then the owner Accepting after Start;
- a run with jsDelivr requests aborted;
- existing specs updated for the read-first default where they assumed an editable Monaco on open.

**Execution:**
- Unit and API suites run locally.
- On this Mac, local `ci-docker` browser phases stop at Phase 4b because there is no `/dev/kvm`. Playwright therefore runs through `just dev-playwright` on the dev stack rebuilt from this branch (needs the user's agreement; no migration), or in the `g5-ci-e2e` lane.
- Anything not executed is reported.

## Risks and assumptions

- **Read-only Monaco replaces the highlight.js viewer for readers and stopped owners.** The comment margin and suggestion zones already work in Monaco; the per-line add control is added. Specs that assumed the old viewer get updated.
- **Bundled Monaco** adds its chunks and workers to the build, loaded lazily. Unit tests mock the new runtime module globally, as they already mock `@monaco-editor/react`, so only the browser run proves bundled loading.
- **Flush snapshots:** a reader may see a flush snapshot whose commit is not a branch head. The base vector names branch heads, as other annotations do, and staleness is judged by blob hash, not by commit.
- **Reviews panel label:** the panel labels every non-round publication by kind instead of "Round N", including existing agent comment annotations.
- **Accept needs the owner's VM,** as for every suggestion today. Reader pushes do not.

## Design review

**Round 1** (Codex `gpt-6-astra`, `xhigh`): CHANGES_REQUIRED, with two blocking findings, both addressed.
1. **In-app navigation could drop drafts without a prompt.** Fixed: drafts now outlive the scoped composer store, in a tab-scoped store keyed by person, workspace, repository and path. In-app navigation keeps them, and `beforeunload` covers closing and reloading. A navigation blocker would need a data router, which is out of scope. `g5.documents.files-view.drafts` was reworded.
2. **NUL-containing UTF-8 could reach Monaco.** Fixed: text now means valid UTF-8 with no NUL byte, checked in FILES for live and durable reads and by the suggestion route. `g5.documents.files-view.text-only` defines this, and `g5.governance.file-suggestions` links to it.

The non-blocking findings were also addressed:
- the stale-start safety reasoning was corrected and moved into the new `g5.governance.file-suggestions.base`, split from `.record`;
- `.stale` now references the display contract;
- `g5.governance.file-suggestions` is tagged `human-provided, human-verified`;
- read-only Monaco comment controls are now specified;
- the test plan was strengthened.

**Round 2:** APPROVE, with no blocking findings. Both non-blocking points are adopted:
- **Combined draft-store tests:** editing, going to Home, then reloading still prompts; a change of person clears drafts; a late Save or Push cannot recreate a cleared draft.
- **Explicit request-body limit:** the route sets its own limit for two texts, with per-text decoded bounds, and a test makes a small edit to a file just under 2 MB.

**Follow-ups** (pre-existing, not changed here):
- `dirtyEditors` keys files by path alone. This design drops every same-path draft on Discard rather than changing that registry.
- The existing anchors to `publish-notifications-after-authoritative-commit` in `documents.g5.md` and `governance.g5.md` point to a heading that no longer exists.

## Review summary

**Design.** The design was challenged against the owning contracts:
- **Reader authority:** the reply precedent and durable reads.
- **Write separation:** pushes create records, not content.
- **Owner Save and acknowledgment.**
- **Suggestion decisions:** reused unchanged.
- **SpecEditor isolation:** a FILES-only seam.
- **Minimal mechanism:** annotation reuse and no migration.

**Implementation.** A self-review ([ontology-review.md](ontology-review.md)) checked each governing commitment against its code path and tests, widened to the affected consumers, and validated the backend's deviations from its brief.
- It fixed four issues: drafts lingering in the mounted editor after Discard or a push, the missing Save my version on changed-on-disk, a reader's Save wording, and missing evidence for threads beside Preview.
- Unit, component, page and API suites, the typecheck, lint and the production build pass; the review lists the exact results.
- **Codex round 1** (`gpt-6-astra`, `xhigh`): CHANGES_REQUIRED with six blocking findings, all confirmed and fixed with regression tests that fail without their fixes:
  - a late push deleting newer edits;
  - a late Save resurrecting discarded text;
  - a push sendable under another person's token;
  - org switches dropping a person's drafts;
  - a clean editor unlisting another file's draft;
  - Preview hiding the source badge.
  Two non-blocking points were also addressed: the oversized-file refusal message and the E2E spec's binary helper, comment and checkpoint checks.
- **Codex round 2:** CHANGES_REQUIRED with one blocking finding, fixed with a regression test. Text typed back to the original while a Save was in flight was lost if the view closed or reopened before the Save settled.
- **Codex round 3:** CHANGES_REQUIRED with two blocking findings from the round-2 fix, both fixed with regression tests: a reopened view left Save disabled for retained text, and a refused Save hid a change on disk.
- **Codex round 4:** CHANGES_REQUIRED with two blocking findings about a Save racing with other writers, both fixed with regression tests. A reopened view kept an obsolete notice after Save my version, and the Save's cache update hid a newer version written after it.
- **Codex round 5:** CHANGES_REQUIRED with one blocking finding, fixed with a regression test: Compare lost merge edits typed before Monaco loaded. Both round-4 fixes passed.
- **Codex round 6**, approved by the user beyond the default cap: **APPROVE** on `e1e60ce4`, with no findings.

Remaining uncertainty:
- **Browser coverage.** The Playwright flows (owner, reader push and Accept, binary card, glyph margin, `beforeunload`, jsDelivr blocked) run in the `g5-ci-e2e` lane of the draft PR [G5Labs/G5-Lean#316](https://github.com/G5Labs/G5-Lean/pull/316); results are pending.

The scoped ontology diff follows.

```diff
diff --git a/.g5/ontology/concepts/core_user_stories.g5.md b/.g5/ontology/concepts/core_user_stories.g5.md
index 54cc722d..234cbb5d 100644
--- a/.g5/ontology/concepts/core_user_stories.g5.md
+++ b/.g5/ontology/concepts/core_user_stories.g5.md
@@ -62,6 +62,7 @@ These links add no separate commitments or implementation percentages.
 - [Read and edit ontology](../features/documents.g5.md#read-and-edit-ontology).
 - [Trace decisions to implementation](../features/documents.g5.md#make-intent-traceability-inspectable).
 - [Discuss a precise line](../features/documents.g5.md#discuss-a-precise-line).
+- [Read a file before editing it](../features/documents.g5.md#read-a-file-before-editing-it).
 - [Synchronize an existing linked view](../features/documents.g5.md#render-ontology-through-the-views-persona).
 - [Turn a document request into ontology intent](../features/documents.g5.md#translate-accepted-document-intent-into-ontology).
 - [Preserve meaning across editors](../features/documents.g5.md#preserve-content-through-rich-editing).
@@ -135,6 +136,7 @@ These links add no separate commitments or implementation percentages.
 
 - [Review in my own workspace](../features/governance.g5.md#review-in-my-own-workspace).
 - [Return a review change](../features/governance.g5.md#return-a-review-change).
+- [Suggest a change to a file](../features/governance.g5.md#suggest-a-change-to-a-file).
 - [Open a pull request](../features/governance.g5.md#open-a-pull-request).
 - [Record a formal review decision](../features/governance.g5.md#record-a-formal-review-decision).
 - [Merge authorized work](../features/governance.g5.md#merge-authorized-work).
diff --git a/.g5/ontology/features/documents.g5.md b/.g5/ontology/features/documents.g5.md
index 56d5eee6..cd8c69e8 100644
--- a/.g5/ontology/features/documents.g5.md
+++ b/.g5/ontology/features/documents.g5.md
@@ -71,6 +71,14 @@ Governing concepts: [Read permission](../concepts/security.g5.md#read-permission
 
 Governing contracts: [a comment keeps its original meaning when code moves](documents.g5.md#a-comment-keeps-its-original-meaning-when-code-moves), [a file's comments follow the file](documents.g5.md#a-files-comments-follow-the-file), [the current owner writes comments](documents.g5.md#the-current-owner-writes-comments), [let readers reply in threads](documents.g5.md#let-readers-reply-in-threads).
 
+### Read a file before editing it
+
+[I · g5.story.read-file-first]
+
+As a workspace reader, I can open a file from FILES as rendered Markdown or as read-only code, the way GitHub shows a repository file, and choose when to edit it.
+
+Governing contracts: [open FILES files for reading](documents.g5.md#open-files-files-for-reading), [render FILES Markdown as GitHub does](documents.g5.md#render-files-markdown-as-github-does), [edit FILES text only as source](documents.g5.md#edit-files-text-only-as-source).
+
 ## Behavioral contracts and design
 
 ### Show the selected durable source truthfully
@@ -230,6 +238,66 @@ Keep ontology navigable across workspace repositories with its source-file and d
 
 Keep Monaco source editing through the ordinary file Save contract.
 
+### Serve the source editor from G5
+
+[D · g5.documents.editor.assets]
+
+Bundle Monaco and its workers with the G5 frontend. Browsers never fetch editor code from a third-party CDN at runtime, so an install without internet access can still edit, and the editor version follows the frontend's own dependencies.
+
+### Open FILES files for reading
+
+[D · g5.documents.files-view]
+
+FILES opens a text file for reading: Markdown in a rendered Preview, any other text file in a read-only Monaco Code view. A requested line, such as a citation or trace jump, opens Code view at that line. Edit is an explicit action that turns the same Monaco view editable in place. HTML files keep their rendered and source modes. These presentations belong to FILES; PROJECT DOCS keeps [block-style spec editing](#retain-block-style-spec-editing) and its [suggestion projection](#limit-suggestion-rendering-to-the-speceditor-entry-point).
+
+### Render FILES Markdown as GitHub does
+
+[D · g5.documents.files-view.preview]
+
+Preview renders GitHub-flavored Markdown: tables, task lists, footnotes, alerts, strikethrough, math and Mermaid diagrams. G5's own document blocks render only in SpecEditor. Raw HTML is sanitized and Mermaid keeps its strict mode. Links to other repository files open FILES tabs, and relative images load from the same selected source. While editing Markdown, Edit and Preview tabs show the unsaved text.
+
+### Edit FILES text only as source
+
+[D · g5.documents.files-view.source-only]
+
+FILES edits only the source text; Preview never accepts typing. Rich Markdown editors re-serialize whole files and rewrote unchanged content in the research tests, so editing in the rendered view awaits a design that preserves untouched bytes.
+
+### Open only text files in the FILES editor
+
+[D · g5.documents.files-view.text-only]
+
+Only text files, meaning valid UTF-8 with no NUL byte, of at most 2 MB open in the FILES editor, whichever source the bytes came from. Any other file shows its name, size and a Download action, with "Too large to display" for a larger text file. Neither offers Edit or Suggest changes, so the editor never writes back bytes it could not show.
+
+### Keep unsaved FILES edits across view changes
+
+[D · g5.documents.files-view.drafts]
+
+Unsaved FILES edits stay in the browser tab, per person, workspace and file, until saved, pushed or discarded. Switching between Preview, Code and Edit, between HTML's rendered and source modes, to another tab or panel, or to another page of G5 keeps them. Closing or reloading the tab while any remain asks first.
+
+### Start compute only for an owner's edit
+
+[D · g5.documents.files-view.owner-edit]
+
+The owner edits through a running attached workspace. When it is stopped, Edit becomes "Start workspace to edit", an explicit [start](../software_architecture/lifecycle.g5.md#explicit-consent-owns-each-start) followed by the [live reload](#reload-before-editing-a-new-attachment); Save keeps its [checkpoint](#publish-an-explicit-editor-save). A reader [suggests changes](governance.g5.md#suggest-file-changes-from-files) instead, without compute.
+
+### Compare before resolving a FILES version conflict
+
+[D · g5.documents.files-view.compare]
+
+When Save meets a newer file version, or [changed-on-disk](#expose-concurrent-disk-changes) appears in a dirty editor, offer Compare beside Reload and Save my version. Compare shows the newer content beside the person's own text, which stays editable, so they can merge by hand before saving against the newer version.
+
+### Keep FILES reading views current
+
+[D · g5.documents.files-view.refresh]
+
+A FILES view without unsaved edits follows its selected source: the owner's live view refreshes on file-change notifications, and a reader's durable view when the workspace's branch, flush or authoritative cut advances.
+
+### Show comments and suggestions beside Preview
+
+[D · g5.documents.files-view.annotations]
+
+Comments and review suggestions stay anchored to source lines. Preview lists the file's threads and pending suggestions beside the rendered page; new threads start in Code view, where suggestion diffs show. This specializes [comment identity](#a-files-comments-follow-the-file) for the rendered view.
+
 ### Retain block-style spec editing
 
 [D · g5.documents.editor.spec]
@@ -560,6 +628,12 @@ Partial incorporation does not terminalize a future contribution session.
 - From [H033](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
 - Partial · the comment contracts below implement it: anchors built from the text on screen, unsaved edits included; one file identity in every view; ordered re-anchoring evidence; the original line shown for threads that cannot be placed. Editor, page, API and resolver tests cover them. The browser flows (`frontend/e2e/flows/comments.spec.ts`) run in the `g5-ci-e2e` lane, which gates the merge; the one for Save, reload and a second reader first passed there on PR #271's `4cf9a823`.
 
+### g5.story.read-file-first
+
+- human-provided
+- The [FILES source editor request](../../chat_intent/files-source-editor/request.md) asks FILES to open files read-first, like GitHub, with editing as a choice.
+- ~90% · delivered by `g5.documents.files-view` and its children through `frontend/src/components/files/FileView.tsx`, verified by vitest component and page tests; the Playwright flows in `frontend/e2e/flows/files-source-editor.spec.ts` are written but not yet executed.
+
 ### g5.documents.read
 
 - agent
@@ -570,7 +644,7 @@ Partial incorporation does not terminalize a future contribution session.
 
 - agent
 - From [C020–C022](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md). Outcome lineage: [v9](../../../docs/lean-backend-design-v9.md).
-- ~75% · reader DTOs and frontend source markers; provider freshness is shown by remote-tracking status rather than a reader marker since the mirror ledger was retired, and complete renderer/source-marker coverage is unassessed.
+- ~75% · reader DTOs and frontend source markers; provider freshness is shown by remote-tracking status rather than a reader marker since the mirror ledger was retired, and complete renderer/source-marker coverage is unassessed. FILES Preview shows the same badge in its header (`SourceBadge.tsx`, `FileView.tsx`; `FileView.test.tsx`).
 
 ### g5.documents.read.live-cache
 
@@ -588,7 +662,7 @@ Partial incorporation does not terminalize a future contribution session.
 
 - agent
 - From the local stale Save review's pending-edit finding (`docs/experiments/runs/2026-09-26-stale-save/final-self-review.md`, omitted from this PR).
-- `CodeEditor.tsx` and `SpecEditor.tsx::persist` retain the submitted content as the Save baseline, leaving later edits dirty; targeted editor tests pass. Other editors remain unassessed.
+- `CodeEditor.tsx` and `SpecEditor.tsx::persist` retain the submitted content as the Save baseline, leaving later edits dirty; targeted editor tests pass. Other editors remain unassessed. A FILES Save that completes after its editor closed, or after its draft was discarded or replaced, acknowledges only the submitted bytes in the draft store and leaves the editor as it is (`updateFileDraft`; `FileView.test.tsx`).
 
 ### g5.documents.save.explicit
 
@@ -606,13 +680,13 @@ Partial incorporation does not terminalize a future contribution session.
 
 - agent
 - From [C019/C137](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md).
-- ~65% · owner-ready editing and live file hooks exist; complete editor reload sequencing and stale pending-Save cancellation are unverified.
+- ~65% · owner-ready editing and live file hooks exist; complete editor reload sequencing and stale pending-Save cancellation are unverified. FILES enables an owner's Edit only once a live read with its VM arrives after "Start workspace to edit" (`FileView.tsx`; `FileView.test.tsx`).
 
 ### g5.documents.save.disk-change
 
 - agent
 - From [C137](../../uplift/g5-lean/candidates/contracts.md) and [v9 §9](../../../docs/lean-backend-design-v9.md). Outcome lineage: [H007/H018](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
-- ~75% · file-watch hooks and source/spec editors; all dirty-buffer/watch/Save races are unexecuted.
+- ~75% · file-watch hooks and source/spec editors; all dirty-buffer/watch/Save races are unexecuted. FILES' `CodeEditor.tsx` (`compareChanges`) shows the notice with Compare, Reload and Save my version under unsaved edits (`CodeEditorFilesOptIns.test.tsx`).
 
 ### g5.documents.files
 
@@ -690,7 +764,67 @@ Partial incorporation does not terminalize a future contribution session.
 
 - human-verified
 - From [H034](../../uplift/g5-lean/candidates/human-decisions.md) and [v9 §9](../../../docs/lean-backend-design-v9.md).
-- ~90% · `CodeEditor.tsx` and `useFiles.ts`; observed-VM Save binding is missing and full browser editing was not executed.
+- ~90% · `CodeEditor.tsx` and `useFiles.ts` carry the observed VM on Save; full browser editing was not executed.
+
+### g5.documents.editor.assets
+
+- human-verified
+- Proposed after code reading showed `@monaco-editor/loader` fetching Monaco 0.55.1 from jsDelivr; the user accepted it with the other defaults in the [FILES source editor request](../../chat_intent/files-source-editor/request.md).
+- ~90% · `frontend/src/components/editor/monacoRuntime.ts` hands `@monaco-editor/react`'s loader the bundled `monaco-editor` 0.55.1 with Vite `?worker` imports, and `loadMonaco.ts` is the only Monaco entry (`CodeEditor.tsx`, `FileCompareDialog.tsx`); a production `vite build` emits the runtime and five worker chunks. Gap: unit tests mock the runtime, and the browser run with jsDelivr requests aborted is not yet executed.
+
+### g5.documents.files-view
+
+- human-provided, human-verified
+- The user asked for GitHub's read-first file experience in the [FILES source editor request](../../chat_intent/files-source-editor/request.md); the per-type defaults, line targeting and in-place Edit are accepted agent proposals.
+- ~90% · `FileView.tsx` (`TextFileView`) from `WorkspacePage.tsx` `case "file"`, with `CodeEditor.tsx`'s `monacoReadOnly` keeping one Monaco view for Code and Edit; `FileView.test.tsx` covers modes, line targets and in-place Edit. Gap: the Playwright flows in `frontend/e2e/flows/files-source-editor.spec.ts` are written but not yet executed.
+
+### g5.documents.files-view.preview
+
+- human-verified
+- The user asked whether Monaco could render Markdown and placed G5-specific rendering in SpecEditor ([request](../../chat_intent/files-source-editor/request.md)); the GitHub-flavored feature list and link and image handling are accepted agent proposals.
+- ~90% · `MarkdownPreview.tsx`: react-markdown with remark-gfm, remark-math and `remarkGithubAlerts.ts`, then rehype-raw, rehype-slug, rehype-sanitize (GitHub's schema, `user-content-` ids) and rehype-katex; Mermaid through the shared strict-mode `MermaidDiagram`; repository links via `repoLinks.ts`; relative images through the same file read API. `MarkdownPreview.test.tsx` covers features, links, images and sanitizing. Gap: parity is by feature list, and styling is not browser-verified.
+
+### g5.documents.files-view.source-only
+
+- human-verified
+- The user agreed to leave rendered-view editing out of the first version after the research found that 63 of 66 rich-editor saves without edits changed the file ([request](../../chat_intent/files-source-editor/request.md)).
+- Implemented · `FileView.tsx` renders Preview read-only and edits only in Monaco; Markdown Edit and Preview tabs show the unsaved text (`FileView.test.tsx`).
+
+### g5.documents.files-view.text-only
+
+- human-provided, human-verified
+- The user limited the FILES editor to text files ([request](../../chat_intent/files-source-editor/request.md)); the 2 MB limit and Download are accepted agent proposals.
+- ~95% · `fileText.ts` `fileTextEligibility` (base64 or a NUL byte is binary; over 2 MiB is too large) routes `FileView.tsx` to a card with Download and no Edit or Suggest; the suggestion route applies `ensure_suggestion_text` to the current file and both texts. `FileView.test.tsx` and `api_workspace_reviews.rs` refusals cover both sides.
+
+### g5.documents.files-view.drafts
+
+- human-provided, human-verified
+- The user asked to fix lost unsaved edits within this effort ([request](../../chat_intent/files-source-editor/request.md)); the leave-page prompt is an accepted agent proposal. Keeping drafts across in-app navigation, so only closing or reloading the tab needs the prompt, answers the first [design review](../../pr_ontology_diff_design/files-source-editor/ontology-plan.md#design-review): the app's `BrowserRouter` offers no navigation blocker, and the scoped composer store resets with the workspace.
+- ~90% · `fileDrafts.ts` (tab store keyed by person, workspace, repository and path; `beforeunload` while any draft exists; a Push review in flight locks the draft in every view), `CodeEditor.tsx` `draft` (restore, mirror, Save completions bound to their editing session and settled in the store, the draft kept in every view while a Save is in flight, release of an ended draft), `useFileDraftsAsDirtyEditors.ts` (the page as its own `dirtyEditors` owner), person-based retention in `RequireAuth` (`useAuth.tsx`) and HTML source-mode restore in `WorkspacePage.tsx`. Covered by `fileDrafts.test.ts`, `FileView.test.tsx`, `useFileDraftsAsDirtyEditors.test.tsx`, `dirtyEditors.test.ts` and the org-switch case in `useAuthContext.test.tsx`. Gap: the real browser prompt and the trip to Home exist only in the unexecuted Playwright flows (`frontend/e2e/flows/files-source-editor.spec.ts`).
+
+### g5.documents.files-view.owner-edit
+
+- human-provided
+- The user limited "Start workspace to edit" to owners, whose Save commits to the branch ([request](../../chat_intent/files-source-editor/request.md)).
+- ~90% · `FileView.tsx` offers Edit only on live bytes of the running attachment and otherwise "Start workspace to edit" (`requestWorkspaceStart`), entering Edit once the live read arrives; `FileView.test.tsx` covers both. Gap: the Playwright flows in `frontend/e2e/flows/files-source-editor.spec.ts` are written but not yet executed.
+
+### g5.documents.files-view.compare
+
+- human-verified
+- Accepted agent proposal in the [FILES source editor request](../../chat_intent/files-source-editor/request.md).
+- ~90% · `CodeEditor.tsx` adds Compare to the Save-conflict dialog (`openConflictCompare`) and a changed-on-disk notice with Compare, Reload and the owner's Save my version, also for a version that arrived during a Save once that Save settles; `FileCompareDialog.tsx` shows the Monaco diff with a textarea fallback; a reader's stale push uses the same dialog (`FileView.tsx`). Covered by `CodeEditorFilesOptIns.test.tsx` and `FileView.test.tsx`. Gap: the Monaco diff editor runs only in the unexecuted browser flows.
+
+### g5.documents.files-view.refresh
+
+- human-verified
+- Accepted agent proposal in the [FILES source editor request](../../chat_intent/files-source-editor/request.md).
+- ~95% · `ws.ts` `invalidateAuthoritativeCut` also invalidates the FILES file query on branch, flush and cut events, and `CodeEditor.tsx` replaces a clean buffer or shows changed-on-disk under edits, including versions that arrive while a Save is in flight; `useSaveWorkspaceFile` never overwrites a newer version read meanwhile. Covered by `ws.workspaceAuthority.test.ts`, `CodeEditorFilesOptIns.test.tsx` and the `FileView.test.tsx` Saves-in-flight cases.
+
+### g5.documents.files-view.annotations
+
+- human-verified
+- Accepted agent proposal in the [FILES source editor request](../../chat_intent/files-source-editor/request.md).
+- ~90% · `FileView.tsx` lists the file's threads and suggestion cards beside Preview through `SideThreads`; read-only Monaco keeps the comment margin and adds a glyph-margin Add comment (`CodeEditor.tsx`). Covered by the `WorkspacePageComments.test.tsx` Preview test and `CodeEditorFilesOptIns.test.tsx`. Gap: the glyph margin in a real browser is in the unexecuted Playwright flows.
 
 ### g5.documents.editor.spec
 
diff --git a/.g5/ontology/features/governance.g5.md b/.g5/ontology/features/governance.g5.md
index c23b735c..b6ba4622 100644
--- a/.g5/ontology/features/governance.g5.md
+++ b/.g5/ontology/features/governance.g5.md
@@ -28,6 +28,16 @@ Governing concepts: Review access follows [read authority](../concepts/security.
 
 Governing contracts: [review in a workspace you own](governance.g5.md#review-in-a-workspace-you-own), [a review returns suggestions to the source owner](governance.g5.md#a-review-returns-suggestions-to-the-source-owner), and [publish existing work under the actor’s provider authority](governance.g5.md#publish-existing-work-under-the-actors-provider-authority).
 
+### Suggest a change to a file
+
+[I · g5.story.suggest-file-change]
+
+As a reader who does not own a workspace, I can edit one of its text files from FILES and push the change to its owner as review suggestions, without a review copy or running compute.
+
+Governing concepts: Suggestion access follows [read authority](../concepts/security.g5.md#read-permission-is-explicit); suggestions are [durable platform records](../concepts/data_model.g5.md#the-api-owns-durable-platform-records).
+
+Governing contracts: [suggest file changes from FILES](#suggest-file-changes-from-files), [a review returns suggestions to the source owner](#a-review-returns-suggestions-to-the-source-owner).
+
 ### Open a pull request
 
 [I · g5.story.open-a-pull-request]
@@ -125,7 +135,37 @@ Explicit Finish stops the review child while dirty or unpushed work remains ther
 
 Push review records a round of suggestions on the source workspace — proposed changes, comments, artifacts and assessments — as durable platform records instead of a pull request. The source owner decides each suggestion and the reviewer may withdraw their own; recording a round moves no Git ref.
 
-This is the hand-back of an [ordinary review workspace](#review-in-a-workspace-you-own). The [scope exclusion](../scope.g5.md#ordinary-reviews-remain-ordinary-workspace-work) of sealed returned results, reviewer pipelines and result-triggered cleanup still stands.
+This is the hand-back of an [ordinary review workspace](#review-in-a-workspace-you-own); a reader can also push one file's changes from FILES as [file suggestions](#suggest-file-changes-from-files) without one. The [scope exclusion](../scope.g5.md#ordinary-reviews-remain-ordinary-workspace-work) of sealed returned results, reviewer pipelines and result-triggered cleanup still stands.
+
+### Suggest file changes from FILES
+
+[D · g5.governance.file-suggestions]
+
+A reader of the source workspace who is not its owner may edit an existing [text file](documents.g5.md#open-only-text-files-in-the-files-editor) in FILES and Push review, whatever the state of the owner's VM and without a review workspace. The server compares the reader's text with the durable version they started from and records the changes on the source as [review suggestions](#a-review-returns-suggestions-to-the-source-owner), with an optional note opening their threads; the owner decides them like any other. Pushing writes platform records only, like a [reply](documents.g5.md#let-readers-reply-in-threads), and grants no workspace-content write; Base, read-only and freezing sources accept none. New files, renames and deletions are not suggested this way.
+
+### Add file suggestions without replacing others
+
+[D · g5.governance.file-suggestions.additive]
+
+Each push adds suggestions: it neither replaces nor is replaced by the reader's earlier pushes or review rounds. To change a pending suggestion, the reader withdraws it and pushes again.
+
+### Push against an older file version deliberately
+
+[D · g5.governance.file-suggestions.stale]
+
+When the selected source's version of the file changed after the reader started, Push review warns and offers Compare. The reader may still push against the version they started from; [placement](documents.g5.md#surface-suggestion-placement-uncertainty) and the [display contract](#show-a-suggestion-the-same-way-everywhere) decide where each suggestion shows.
+
+### Record a file suggestion push as its own annotation
+
+[D · g5.governance.file-suggestions.record]
+
+Each push is a new annotation publication with no review workspace, holding the computed hunks with their per-edit identities; its base vector names the source heads at the push. A push whose only change is the final newline proposes nothing and is refused, as in review rounds.
+
+### Take a push's starting version from the reader's text
+
+[D · g5.governance.file-suggestions.base]
+
+The client returns the text the reader started from, and the server compares that text's Git blob hash with the reader's current durable file to detect a stale start. It never reads a commit or blob the client names, because repository storage can be shared between workspaces. A deliberately stale start is the reader's own unverified text: its suggestions may still place where their text matches, but like every suggestion they change nothing until the owner accepts them.
 
 ### Show a suggestion the same way everywhere
 
@@ -476,6 +516,12 @@ Automatic divergence pauses synchronization without launching resolution. An exp
 - From [v9](../../../docs/lean-backend-design-v9.md), [source lineage](../../uplift/g5-lean/expansion/stories.md#us52--return-a-review-change). [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) replaced pull-request hand-back with Push review.
 - Partial · `routes/workspace_reviews.rs` Push review and `workspace_review_service.rs` return suggestions to the source; the pull-request hand-back and Finish paths no longer exist. Reassessed for thread and display paths only; round, race and recovery behavior is unexecuted.
 
+### g5.story.suggest-file-change
+
+- human-provided
+- The user asked that a non-owner's FILES edits become suggestions to the owner, with no fork and no VM ([request](../../chat_intent/files-source-editor/request.md)).
+- ~90% · delivered by `g5.governance.file-suggestions` through `FileView.tsx` and `routes/workspace_reviews.rs::publish_file_suggestions`, verified by API and vitest tests; the Playwright flows in `frontend/e2e/flows/files-source-editor.spec.ts` are written but not yet executed.
+
 ### g5.story.open-a-pull-request
 
 - human-verified
@@ -545,8 +591,38 @@ Automatic divergence pauses synchronization without launching resolution. An exp
 ### g5.governance.review-suggestions
 
 - agent
-- Recovered from merged [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) (kaihuiz-g5) and migration `021_review_suggestions.sql`; retained because the [comments design request](../../chat_intent/comments-design/request.md) asks for its threads to work. Its fit with [H081](../../uplift/g5-lean/candidates/human-decisions.md#h081--returned-result-reviewexplainer-product-workflow-removed-from-mvp) is a scope reading no human has confirmed.
-- Unassessed beyond threads · `backend/g5-api-core/src/services/workspace_review_service.rs`, `routes/workspace_reviews.rs`, the review handlers in `routes/internal.rs` and `ReviewsPanel.tsx` implement rounds, decisions and cards; only thread and display paths were inspected.
+- Recovered from merged [PR #170](https://github.com/G5Labs/G5-Lean/pull/170) (kaihuiz-g5) and migration `021_review_suggestions.sql`; retained because the [comments design request](../../chat_intent/comments-design/request.md) asks for its threads to work; the [FILES source editor request](../../chat_intent/files-source-editor/request.md) adds file suggestions as a second producer. Its fit with [H081](../../uplift/g5-lean/candidates/human-decisions.md#h081--returned-result-reviewexplainer-product-workflow-removed-from-mvp) is a scope reading no human has confirmed.
+- Unassessed beyond threads · `backend/g5-api-core/src/services/workspace_review_service.rs`, `routes/workspace_reviews.rs`, the review handlers in `routes/internal.rs` and `ReviewsPanel.tsx` implement rounds, decisions and cards; only thread and display paths were inspected. FILES pushes reach the same rows through `publish_file_suggestions` and the hunk conversion it shares with rounds (`hunk_suggestions`).
+
+### g5.governance.file-suggestions
+
+- human-provided, human-verified
+- The user specified Suggest changes on the owner's branch with no fork or VM, and Push review in place of Save ([request](../../chat_intent/files-source-editor/request.md)); limiting it to existing text files is an accepted agent proposal.
+- ~95% · `routes/workspace_reviews.rs::publish_file_suggestions` admits a non-owner reader of a source that accepts thread writes (`admit_file_suggestions`, checked again in the recording transaction), reads the current file as that reader reads it (`read_workspace_file_for_actor_bounded`) and records hunks through `workspace_review_service.rs` (`file_suggestion_changes`, the shared `hunk_suggestions`, `insert_generated_suggestions`), notifying the owner once; `FileView.tsx` offers Suggest changes and Push review (`usePushFileSuggestions`). Covered by seven `api_workspace_reviews.rs` tests and `FileView.test.tsx`. Gap: the Playwright flows in `frontend/e2e/flows/files-source-editor.spec.ts` are written but not yet executed. Push review is bound to the rendered principal, so another person's token never sends it (`usePushFileSuggestions`; `FileView.test.tsx`).
+
+### g5.governance.file-suggestions.additive
+
+- human-verified
+- The user chose option (a), adding suggestions on each push ([request](../../chat_intent/files-source-editor/request.md)).
+- Implemented · each push creates its own annotation publication, which no later push or round supersedes; `file_suggestion_pushes_add_up_and_no_round_supersedes_them` fails when the kind is `round`.
+
+### g5.governance.file-suggestions.stale
+
+- human-verified
+- Accepted agent proposal in the [FILES source editor request](../../chat_intent/files-source-editor/request.md).
+- Implemented · a stale start returns `FILE_VERSION_CONFLICT` with the current hash, and `FileView.tsx` offers Compare and Push anyway; `allowStaleBase` records against the reader's text, which places only where it still matches. Covered by `a_stale_file_suggestion_conflicts_until_pushed_anyway_and_places_only_where_its_text_matches` and the `FileView.test.tsx` stale tests.
+
+### g5.governance.file-suggestions.record
+
+- agent
+- Agent design in the [FILES source editor plan](../../pr_ontology_diff_design/files-source-editor/ontology-plan.md): a `round` would supersede the reader's earlier pushes, and `current_publication_for` could merge a push into a round.
+- Implemented · `publish_file_suggestions` creates an `annotation` publication without a review workspace whose base vector is `annotation_vector(source_heads)`; `file_suggestion_changes` refuses a final-newline-only change; the API tests cover both.
+
+### g5.governance.file-suggestions.base
+
+- agent
+- Agent design in the [FILES source editor plan](../../pr_ontology_diff_design/files-source-editor/ontology-plan.md): the commit readers in `workspace_reader.rs` accept any commit in a shared repository; the first [design review](../../pr_ontology_diff_design/files-source-editor/ontology-plan.md#design-review) corrected why an unverified stale start is acceptable.
+- Implemented · the handler compares `GitRepo::blob_id_for_bytes(baseContent)` with the hash of the reader's durable read and reads no client-named commit or blob; covered by the stale and flush-snapshot API tests.
 
 ### g5.governance.review-suggestions.display
 
```
