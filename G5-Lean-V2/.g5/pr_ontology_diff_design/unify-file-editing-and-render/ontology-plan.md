# Shared file editing and read-only previews

Work for [issue #793](https://github.com/G5Labs/G5-Lean/issues/793), on the assigned
`unify-file-editing-and-render` branch. Implementation is authorized by the request
to work on the issue. No new backend Save API or collaboration protocol is introduced. Current document writes move from the Docs route to the existing Files PUT, retaining its common write service and preconditions.

## Governing decisions

[File identity](../../ontology/features/documents.g5.md#g5.documents.identity),
[editor ownership](../../ontology/features/documents.g5.md#g5.documents.editor.ownership),
[source engine](../../ontology/features/documents.g5.md#g5.documents.editor.engine),
[shared Markdown](../../ontology/features/documents.g5.md#g5.documents.markdown.shared),
[opening defaults](../../ontology/features/documents.g5.md#g5.documents.source-editing),
[drafts](../../ontology/features/documents.g5.md#g5.documents.files-view.drafts),
[selected Save](../../ontology/features/documents.g5.md#g5.documents.selected-save),
[discussion authority](../../ontology/features/documents.g5.md#g5.documents.comments.authority)
and [question decisions](../../ontology/features/documents.g5.md#g5.documents.question-decisions)
govern this change. Ontology keeps its read-only details, reference popovers and
[statement comparison](../../ontology/features/documents.g5.md#g5.documents.ontology.statement-changes).

Resolve document identifiers to their actual repository/path before opening a current
file. Files, Project Docs, Ontology, citations and deep links then focus one tab.
Revision views remain separate and read-only. A browse opener does not reduce or
increase file authority: the current workspace, person, source and VM determine it.
Never infer a repository from a same-named path in another repository.

Extend the existing browser-tab fileDrafts owner with retained Monaco models and
view state. The key includes person, workspace, repository and canonical path.
Navigation and Source/Preview switches retain native history; successful Save changes
the acknowledged baseline, not the model or later typing. Serialize Save per draft,
including a remounted view. Stale completions cannot recreate discarded sessions.
Person changes dispose models as well as drafts. Authority loss keeps one's recovery
text but disables direct writes; an owner draft is never silently converted into a
reader suggestion. Explicit discard ends its history. These are in-memory sessions,
not new durable storage; the existing unload warning applies to unsaved text.

FileView owns the common controls and uses CodeEditor for every source file.
Preview is the initial Markdown view; Edit opens Source, and line citations open
Source directly. Save and recovery remain available while Preview is selected.
Reuse existing file/document read adapters only where necessary; all writes retain
the selected-file Save preconditions and publication result. HTML remains sandboxed,
JSONL keeps its transcript presentation, and comparisons retain Monaco DiffEditor.

Extend the existing react-markdown/remark/rehype Files renderer as the shared Markdown
pipeline. A read-only document layer derives semantic frames, metadata, outlines,
questions and comparison rows from source ranges; it never serializes parsed Markdown
back into the source. Preserve GFM, math, Mermaid, relative links/images, anchors,
sanitization and trusted/untrusted widget boundaries. Reuse domain question controls,
comment owners, review projections/cards and reference navigation. Domain actions are
explicit operations; merely displaying a preview is pure. Remove SpecEditor and its
rich input, structural insertion/deletion and custom history, rather than retaining a
second editing engine hidden behind a flag. Chat composition stays outside this work.

## Capacity and verification

The existing Files 2 MiB cutoff must not reduce the document/source envelope. Retain
the shared read service's 32 MiB admission; measure Monaco and preview behavior with
representative larger input before selecting any preview-specific load policy. Do not
evict dirty models or silently truncate content. Preview-only work can be deferred
behind an explicit control when measured cost warrants it; Source remains available.

Verify canonical navigation and repository isolation; exact submitted bytes, CRLF,
Unicode, IME and native undo through navigation; delayed/failed saves and later typing;
external changes, VM replacement, discard, role changes and person changes; questions,
comments, reviews, source anchors and historical links. Run affected unit tests,
frontend build/lint and browser journeys with real Monaco. Record limitations of
mocked backend evidence and unexecuted integration checks here before handoff.

## Self-review

Reuse the existing draft owner, selected-file Save, comment store, question controls
and review projection. A second session store or Save API would duplicate lifecycle
and authority. Keeping rich editing would defeat the requested consolidation.
The material migration risk is losing a domain control or source location while
removing SpecEditor; explicit preview and navigation regressions cover those consumers.
The issue supersedes earlier ontology-only rich insertion/deletion requirements;
read-only presentation and provenance rules remain in force.

## Implementation and measured capacity

Current openers, including restored repository-derived document handles outside the
current Docs listing, converge in `currentFileTab`; `FileView` supplies the shared toolbar,
source session and read-only domain preview. The old rich editor and structural
insertion/history code are removed. Generic and ontology statement Markdown use the
same sanitized pipeline. Preview comments retain their canonical source anchors in
the shared discussion list; starting a record comment opens its source line. Review
alternatives do not move or rewrite that source. Embedded question answers rewrite
only the expected fenced payload and its derived legacy label tags into the shared
draft, then use explicit file Save. Current ontology markers and metadata stay exact.
Referenced question sets keep their existing command API and authority checks.

Chromium measurements used the production FileView/Monaco components in a Vite
fixture with generated repeated Markdown. These are local development measurements,
not production latency guarantees. An initial automatically rendered preview took
about 0.55 s at 64 KiB, 1.65 s at 256 KiB, 19.45 s at 1 MiB and 68.84 s at 2 MiB;
the 2 MiB DOM reached about 64,000 nodes. These results originally led to an explicit
**Load preview** action above 128 Ki UTF-16 code units, raised by the
[2026-11-09 follow-up](#preview-threshold-follow-up--2026-11-09). This threshold
bounds eager rendering, not file access or editable source size.

After deferring preview at the original threshold, an isolated browser run measured:

| UTF-8 ASCII file | Open deferred preview | Open Monaco Source | Browser JS heap |
| --- | ---: | ---: | ---: |
| 1 MiB | 32 ms | 339 ms | 53.5 MB |
| 8 MiB | 126 ms | 439 ms | 116 MB |
| 32 MiB | 454 ms | 978 ms | 225 MB |

Each Source model contained the entire input (including 33,554,432 characters in the
32 MiB case); the fixture DOM stayed around 385 nodes. Source admission matches the
existing 32 MiB read envelope, replacing the earlier Files-only 2 MiB cutoff. Models
and undo history are retained for the browser-tab session, with no automatic dirty
model eviction. Explicit discard resets history; changing person disposes models.
Memory grows with opened models and their history; reopening the browser ends this
in-memory session and the existing unsaved-draft unload warning remains in force.

Validation uses unit/integration regressions plus real Chromium Monaco journeys.
The browser fixture controls delayed/refused backend responses and captures the exact
Save request; it does not exercise a live VM write, Git commit or remote push. Existing
backend selected-file write service is reused, but the HTTP route changes from Docs PUT to Files PUT. The owner review found its default 2-MiB request limit; Files PUT now explicitly preserves Docs’ 32-MiB JSON envelope. This is a request-body limit, so JSON escaping and field overhead count toward it. Native browser checks cover CRLF,
Unicode, IME, undo/redo through remount and Save, delayed Save with later typing,
failure recovery, access/person/role/VM changes, question and comment keyboard actions,
review draft preservation, and source opening above the old Files size cutoff.

The completed browser run passed all nine source/domain journeys, plus a GFM browser
check for list markers, disabled task checkboxes, tables, math and preview purity.
Explicit shared list styles preserve markers against Tailwind's global reset. The workspace-level integration
suite passed all 55 comment/recovery/authority cases and the seven current-document
opener cases. Shared-renderer link regressions preserve qualified in-app navigation,
restricted/historical inert links and hostile-attribute rejection. Removed obsolete
DOM-to-Markdown and structured-mutation helpers after migrating their remaining useful
read-only assertions. A visual inspection found the existing Monaco color converter
dropped overlay alpha; retaining alpha fixes the active-line contrast in the newly
shared Source surface, covered by three theme cases and a native browser rerun.
All 4,688 frontend tests across 317 files passed. The first full command was interrupted
after 168 passing files; the remaining 149 files passed in a separate run. Production
build and lint pass (lint reports 484 warnings and no errors); published-head CI
results are recorded on PR #798. Browser backend limitations
above still apply.


The follow-up review fixes preserve unresolved repository ownership for legacy tabs,
reveal the comment margin when a Preview record starts a draft, and leave unmatched
nested/setext headings with the shared renderer's unique IDs. Removed an unreachable
list-specific review renderer; ordered review blocks use the shared Markdown pipeline.
The four affected suites passed 228 tests after the new list assertion was corrected
for whole-block review presentation; another 247 affected editor/workspace cases passed.
All ten Monaco Chromium journeys passed again, including a previously hidden comment margin.

The first published end-to-end run exposed flows and a browser fixture that still
referenced the retired rich editor. Those consumers now use the common
Source/Preview interface, preserving durable Save/fork, conflict, reference, comment,
review and native undo assertions. All eight review-layout Chromium cases pass.
CI also exposed duplicate article semantics when embedding the shared renderer in a
timeline entry; its wrapper is now neutral and in-page anchors stay scoped to that
preview. Live-stack results are recorded on PR #798.

The migrated live-flow helpers also passed a native Chromium journey with authored
I/D/Q records, Mermaid, repeated Source/Preview switches and undo after both appending
and deleting source. The latest local set is eleven Monaco journeys plus eight review
layout journeys. The renderer/timeline regression run passed 94 cases; TypeScript passes.

The published-head run at `92595de85` passed all 4,694 frontend tests and the full
Rust job. Live browser coverage caught the HTML answer bridge requesting an explicit
Save checkpoint after consolidation. The shared file callback now carries the existing
`commitAndPush` option: HTML Apply passes false, while Source Save defaults to true.
The same draft/save lock and hash/VM preconditions govern both. An integration regression
checks the two HTTP payloads in sequence. The legacy Doc deep-link flow reads its handle
by the already opened canonical path, rather than guessing it from a display name.
Live-flow selectors now follow the canonical block wrapper and filename-based HTML
preview title. The follow-up passed 56 targeted tests, TypeScript, the production build,
and collection of all 937 browser cases; targeted lint reported no errors.

The next live run passed all 4,695 frontend tests and the Rust check. It also
verified the independent Project Doc review flow, then exposed a second HTML
bridge transition: a draft can reload its iframe while Save is pending. The
bridge now announces changed capabilities after settlement or permission changes
without resetting its generation. A real iframe/Monaco browser regression failed
before the fix and passed afterward; all twelve file-session browser journeys
and 78 targeted tests pass, as do TypeScript, the production build and targeted lint. The legacy Doc
link assertion uses the authoritative display name returned by its path read.

All required CI checks passed at `25d3fb5ec`, including 4,695 frontend tests,
the Rust check and all seven end-to-end stages (299 browser cases passed, 13
skipped). Base then advanced to `bc38e10c6` with the Aero Glass fixes from #799.
The linear rebase preserves its tab styling and short-hex normalization alongside
this work's translucent Monaco overlays; both theme test groups remain. No
governing ontology changed on the parent. The reconciled theme and tab tests
passed all 35 cases, with clean targeted lint. All twelve file-session browser
journeys and the production build also passed after the rebase.

## Editor consistency and dead-code follow-up

The owner's preview check on 2026-10-25 requests a second pass for editor divergence
and dead code. Trace current openers and remaining preview adapters against the same
file-session contract. Remove unused rich-mode controls, structural write helpers and
selectors after checking their consumers; preserve dedicated domain forms and fallback
source access. The question preview currently bypasses the existing label-tag writer:
restore answer/clear/edit synchronization through that writer while retaining guarded,
exact source splices and CRLF. Resolve legacy document tabs before content dispatch so
HTML, JSONL, questions and reviews cannot diverge through a second FileView adapter.


The follow-up also routes document-change notifications and sync to the shared
workspace-file cache, retaining cancellation of stale in-flight reads. Source/Preview
selection joins the existing person/file session so reopening a file preserves it.
Explicit Edit opens Source; completion/discard returns to the normal reading view.

Base `18c4bdd799` contains compatible editor design commitments, durable design-file
visibility, personal workspace navigation and retirement of agent review comments.
Reconcile the document ontology around Base's stable editor ownership/modes/engine
and shared-Markdown IDs, retaining this plan's measured capacity decision and
opening defaults. Preserve Base's new content visibility and route behavior;
human suggestions/comments and their existing authority remain supported. The
retired SpecEditor stays deleted. The selected-view browser regression reproduced
its reset before the fix; the question-tag regressions reproduced the missing label
updates before the shared writer was restored.


The cleanup checkpoint passed thirteen native Monaco journeys, eight review-layout
journeys and 167 focused session/projection tests, alongside the earlier 274 and 61
consumer regression runs. The broad run begun before the last edit passed 4,697
cases but cached the old module for one newly added view-retention test; a fresh
167-case run passes that test. Rebased validation uses a stable checkout. TypeScript
passed before rebasing. No backend mutation or discussion authority changed.


Reconciled-head validation passed all 4,786 frontend tests across 323 files, thirteen
native file-session journeys, eight review-layout journeys, TypeScript and lint
(0 errors). The production bundle passed with a 4-GiB local Node heap after the
2-GiB default exhausted it; CI's frontend build and all frontend tests passed too.
The Rust check passed, including all 1,792 API integration cases. Live CI passed
Core/Compare/All Changes navigation and comment/reply flows, but the interactive
comparison test matched both the visible preview and its hidden retained Source
copy. Scope that assertion to Preview and add a native Compare/Save/Undo journey
which also checks that Source stays hidden while reading. This corrects test
ambiguity without changing the retained source/view behavior. Published-head CI
results remain on PR #798.

The added native comparison journey passes, and the complete file-session browser
set passes all fourteen cases. All 940 live-browser cases collect successfully.
The first reconciled live run completed 298 passing journeys and 13 skips; its
only failed journey was the ambiguous comparison assertion above (three attempts).
The correction and additional browser fixture change no production behavior.


## Owner review disposition — 2026-10-25

The [owner’s review](https://github.com/G5Labs/G5-Lean/pull/798#issuecomment-6393890252)
is evaluated against the latest branch, including fixes already present after the
reviewed `47d36b10b` head.

1. Restore the 32-MiB Files PUT envelope and gate Suggest on the backend’s existing
   2-MiB UTF-8 base/proposal limit. A real API test writes 3-MiB HTML through the
   scripted runtime peer and checks the inclusive request boundary. Oversized suggestion
   drafts remain recoverable. The previous source-size browser measurement did not
   verify HTTP admission; the capacity statement above is corrected.
2. The duplicate Doc FileView, Docs-list prerequisite and unused Save hook were
   already removed. Current handles resolve to the shared file tab before dispatch.
3. Remove remaining unused tab, renderer, review and CodeEditor flags. Keep the
   User Guide’s live `basePath` and heading-ID adapter, and the restored question
   writer/Human-filter helper. Keep path-based legacy `.semlift.md` review metadata;
   remove its unused explicit-component and doc-type parameters. Wire Repair payload
   through the same exact-source rewrite used by question answers.
4. Share draft construction, conflict extraction and guarded session writes;
   consolidate frame metadata, label matching, risk classification, alert labels
   and repository-handle parsing. Source Save, HTML Apply and question rewrites
   join the same draft; Save keeps the existing per-session reservation.
5. Rename retired-editor selectors and browser flow; move Markdown link tests next
   to the renderer and update current ontology evidence.
6. Preserve the accepted side-list/Source-jump comment design. Preview no longer
   offers rich selection commenting, inline highlights or Unresolved filtering.
   Archive opens follow actual file authority; eligible readers can suggest on
   Docs/ontology. Statement images use the shared image renderer. Collapsed-Boundary
   review guidance and the unreachable Edit document text action are retired.
   Question repair is restored, not removed. These differences are explicit in the
   governing document ontology and PR description.
7. Replace the compatibility fixture with direct DocumentPreview props and assert
   actual `onRewrite` calls. Test both sides of the 128-Ki code-unit boundary and
   the explicit Load preview action. Direct tests also require Compare to reset
   for a different document without a wrapper supplying that reset.
8. Extract HtmlPreviewShell from WorkspacePage. The dedicated native Monaco/review
   fixture journeys remain local-only: file sessions use `npm run test:e2e:mock-preview`
   and review layout uses `playwright test -c e2e/browser/playwright.config.ts`; CI’s
   default Playwright project covers live workspace flows separately.
9. Keep the small, separately called-out pre-existing Monaco overlay-alpha fix,
   including #799’s short-hex support. Tests cover authored/translucent/minified
   tokens and native editor rendering; it is not a requirement of editor removal.
10. Keep explicit same-repository reference resolution, with a regression that a
    qualified link works without a branch-list read. Other repositories still need
    unique authorized resolution; missing or ambiguous repositories remain errors.
11. Keep question-metric invalidation after acknowledged shared file saves, now
    scoped to aggregates containing that workspace. Test success, refusal and
    unrelated aggregates. It replaces the invalidation formerly owned by Doc Save;
    typing does not invalidate metrics.
12. Defer the larger useFileSession/model-only-buffer and clean-model release
    redesign. It changes lifecycle ownership beyond this migration’s bounded fixes.
13. Defer the AST-only parser/heading-ID migration. Preserve the live User Guide
    adapter and explicit ontology anchors; heading-slug convergence needs separate
    compatibility design and tests.
14. Defer the DocumentPreview hook split and indexed parser joins/performance work.
    Fix the bounded outline-root issue here and test duplicate block indices outside
    the preview, including a target removed by filtering. The measured explicit
    Load preview threshold remains the mitigation for large Markdown work.


Review follow-up validation: the HTTP admission regression passes through the real
API route and common write service with a scripted runtime peer. The large write
asserts exact stored bytes; the request boundary test refuses an admitted request
on its VM precondition and rejects the next byte with HTTP 413. This is not a real
VM/Git-publication test. Fourteen native Monaco journeys and eight review-layout
journeys pass; the review fixture now imports DocumentPreview directly too. Focused
runs pass 144 preview/draft cases and 121 sidebar/Markdown cases. Production build
and full lint pass (no errors). The first broad unit run overlapped final sidebar
cleanup and also exposed tests that equated read-only authority with the loading
fallback; those tests now explicitly control module loading and cover read-only
Monaco as well. Final published-head CI remains the stable full-suite evidence.

A fresh local full run passes all 4,800 frontend tests across 324 files. Live
browser assertions now check the canonical file path and repository instead of
the retired `editor=ontology` hint. CI subsequently exposed an intermittent
suggestion-card test failure while source verification was pending. That test
now controls verification completion, queries the current card, and checks that
the replacement draft survives verification and regrouping; all 97 related
suggestion-preview cases pass. These final corrections change tests only.

## Main rebase — 2026-10-26

Rebase the existing linear series onto `44bf3c0bd7fc6701cbee0addde9b1697eb895cf8`.
Main's code-block Copy contract composes with the shared preview: one code renderer
keeps its exact clipboard text, compact header and failure feedback across document,
chat and restricted surfaces. Migrate the new SpecEditor copy tests to DocumentPreview.
Preserve main's scoped read caches, live-tab authority expiry, restored-route handling,
Source Control checklist/checkpoint rules, windowed comparisons and tab reordering.
Canonical document-to-file conversion carries the live-authority marker so restoring
a tab cannot bypass its expiry. Adapt main's delayed-save isolation regressions to
the common file hooks instead of retaining the retired document hooks. Keep HTML
Apply's no-checkpoint policy and the
shared file Save path. Current-main landing and Activity behavior remain intact.

The previous CI results above describe the pre-rebase head. Rebased validation and
published-head checks are recorded on PR #798.

The CI follow-up rebases this series onto main
`280aaa157fc47c353b860a9e0da1c96a9f3aa228` without adding merge commits. Main's
zstd 0.14.3 / zstd-safe 8.2.0 / zstd-sys 2.1.2 upgrade addresses the four security
advisories from the previous Rust run and preserves stored session-body compatibility.
There are no new ontology or editor conflicts. The previous formal-review scenario
hit nextest's 720-second guard without emitting a failing review state; the same
scenario passed in 165 seconds on main's green PR #840 run. Validate the rebased
published head before attributing that timeout or changing its test budgets.

## Shared preview wrapping regression — 2026-10-27

After #798 merged, the owner reported forced line breaks. Browser reproduction
finds that the common renderer inherited FILES' former 980px maximum width and
16px/24px/32px page padding. Those rules now affect chat, statement bodies,
historical previews and metadata too: a 1,400px host leaves only 932px for prose.
Ordinary Markdown source newlines still collapse into spaces; the reproduced
regression is premature visual wrapping and unintended embedded-page padding.

The owner clarified that Markdown must fill the window space remaining after
sidebars and padding, with wrapping at the content edge. Apply the explicit
shared contract `width: 100%; max-width: none; min-width: 0; box-sizing: border-box`.
Remove the assistant turn's 95% ancestor cap and the FILES JSONL transcript's
`max-w-4xl` cap. The actual Files/Core document columns already fill the space
beside their outline. Six additional browser cases reproduce the ancestor caps
and cover actual center chat, side-panel chat and transcript previews, with and
without a sidebar. They compare prose with the pane after real spacing instead
of only its immediate parent, and check resizing and horizontal overflow.

Restore the existing host's ownership of column width and outer spacing. Keep
document page padding on DocumentPreview's outer Markdown surface, with no
shared width cap or nested page inset. Preserve parsing, intentional hard breaks,
paragraphs, code whitespace, editing and comparison behavior. Five real-browser
layout cases reproduce the failure before the fix; they also check wide/narrow
containers, soft newlines, both Markdown hard-break forms and exact code text.

The first width correction passed all thirteen layout browser cases (including
the actual Docs and Core Ontology FileView surfaces), thirty-one existing chat, code-Copy and
suggestion-layout browser cases, 425 affected unit tests and TypeScript. Targeted
lint reports no errors and nine existing warnings. The five embedded-renderer
cases fail on the old CSS and pass with the correction; all six actual-chat and
transcript cases also fail before removing their ancestor caps. The chat browser
scenario now scrolls to load earlier history, preserving its history and disclosure
assertions when a short transcript has already triggered automatic pagination.
Published-head CI is recorded on the follow-up PR.

The owner's acceptance follow-up identified pre-existing caps left outside that
coverage: user and system chat messages, issue details, workspace activity and
its header, and retained agent review reports. Remove those caps as part of the
same available-width requirement. User bubbles may remain compact for short
messages and grow to the full content edge for long ones. Keep their alignment,
page padding and the activity timeline's timestamp/icon columns. The standalone
issue page header follows the same full-width column as its issue body.

Extend the production-component browser fixture to user/system messages in both
chat layouts, embedded and standalone issue details (including comments), activity
header and comment body, and the retained report page. All twelve added cases
reproduce the prior head's caps in wide panes and pass after their removal;
they also check resizing and available space beside sidebars. The pane-width
measurement reserves real timeline columns while still detecting centered
maximum-width margins. This follow-up passes all 25 layout browser cases and
31 existing chat, code-Copy and suggestion/reply browser cases, 406 affected
unit tests and TypeScript. Targeted lint has no errors and eight existing
warnings. Published-head CI is recorded on PR #849.

## Preview threshold follow-up — 2026-11-09

For [issue #1180](https://github.com/G5Labs/G5-Lean/issues/1180), the owner requested
raising the shared preview threshold to 1 MiB, and confirmed that Markdown, HTML,
JSONL and SVG previews must share it. All four already pass through `FileView`'s
common deferral check. It continues to measure the displayed source in UTF-16 code
units: exactly 1,048,576 units render automatically;
larger content requires **Load preview**. This equals 1 MiB for ASCII text, not a
UTF-8 byte limit. The findings-file exception, explicit loading, Source access and
Rich text's existing deferral behavior remain in place.

This is an owner-selected threshold, not a newly measured latency budget. The
production corpus, re-render, Source responsiveness, Rich text and throttled-device
measurements requested by the issue remain outstanding; the development timings
above describe the earlier implementation. Boundary tests cover exactly the new
threshold and one unit above it with all four production preview renderers,
including explicit loading and complete Source access. The findings regression
exceeds the new threshold.

Validation: all 91 `FileView.test.tsx` tests pass. The earlier Chromium journey
that opens and edits 3-MiB Markdown in Source while Preview is deferred also passed.
The frontend TypeScript check passes; targeted lint has no errors and one existing
Fast Refresh warning. These functional checks do not establish a latency budget.

## HTML preview exception — 2026-11-09

After PR #1188 merged, the owner requested a [preview-gate exception for four HTML
extensions](../../chat_intent/raise-file-preview-size-limit/html-preview.md), first
auditing the comparison with PDF/PNG/JPEG. That supersedes HTML's inclusion in the
threshold follow-up above; Markdown, JSONL and SVG retain the 1-Mi-code-unit gate.

The audit traced `fileViewerKind` and `FileView`: PDF and raster images open their
dedicated viewers before the text preview gate. They have no corresponding
file-size deferral, but both committed reads (`workspace_reader.rs` and
`workspaces.rs`) and live reads (`g5d-dispatch/src/files.rs`) refuse files above
32 MiB before returning their bytes. `ImageFileView` also rejects images whose
recognized headers declare more than 64 × 1024² pixels; `PdfFileView` renders
nearby pages and reduces each page's output scale to fit 16 × 1024² canvas pixels.
The PDF cap limits raster resolution, not file size or page count.

`g5.documents.editor.bounds` owns the exception; read bounds, editor modes,
source editing, text measure and viewer commitments continue to govern it. Reuse
the existing HTML iframe and draft lifecycle. One shared, case-insensitive HTML
path predicate selects `.html`, `.htm`, `.xhtml` and `.shtml` for both routing and
the deferral exception, and keeps their Source unwrapped. The latter two suffixes
previously opened as ordinary text; all four now use the same HTML preview and
Source language. XHTML uses the existing HTML parsing behavior and SHTML is
displayed without server-side include processing. The iframe retains its
`allow-scripts` sandbox without same-origin privileges and its existing bridges.

The exception admits no new bytes: binary/NUL-containing HTML and reads above
32 MiB retain their existing refusal behavior, and owner writes and reader
suggestions retain their limits. HTML above 1 MiB must mount automatically and
retain complete Source; Markdown, JSONL and SVG must still defer above the
threshold and load on explicit action. Check all four HTML suffixes through
the real workspace dispatcher, case handling, read eligibility, source wrapping,
and a browser journey rendering and editing 3-MiB HTML. Existing sandbox/answer
bridge tests cover its unchanged authority and refusal behavior.

Large or script-heavy HTML can consume substantial browser resources. The owner
selected automatic preview; functional verification does not establish a
production or throttled-device performance budget, which remains unverified.

Validation: 258 tests pass across `FileView`, the workspace dispatcher, file
eligibility, text measure, sandbox frame/CSP and answer bridge suites. Chromium
passes the 3-MiB HTML render/edit journey, the existing delayed HTML answer-save
and permission-recovery journey, and the 3-MiB Markdown deferral journey.
TypeScript passes; targeted lint reports no errors and 84 existing warnings in
the large workspace page/test files and FileView. The first browser run hit a
cold-start timeout in the existing answer journey and caught an extra test-only
mode switch after Done; the corrected, warm run passes all three cases.
