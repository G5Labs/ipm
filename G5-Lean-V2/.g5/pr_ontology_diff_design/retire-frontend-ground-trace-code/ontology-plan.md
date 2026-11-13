# Retire legacy review metadata

The owner requested implementation of [issue #850](https://github.com/G5Labs/G5-Lean/issues/850),
focusing on the comment's [Proposed removal plan](https://github.com/G5Labs/G5-Lean/issues/850#issuecomment-6434584909).
An uploaded Project Doc is available through the live file API while the obsolete
review-metadata reader requires committed content. Removing the retired feature
eliminates that dependency for both empty and nonempty documents.

## Governing behavior and ownership

[Legacy review retirement](../../ontology/features/documents.g5.md#g5.documents.legacy-review-retirement)
owns the removal. DocumentPreview keeps the shared Markdown pipeline and source-derived
Human/Risk filters, question controls, comments and suggestion overlays under the
existing [editor ownership](../../ontology/features/documents.g5.md#g5.documents.editor.ownership).
SemanticRecordFrame retains authored statement details and question/provenance accents.
Neither component fetches implementation assessments or selects legacy actions.

The owner's preview check found that the retained Human and Risk/Question filter
buttons still appeared on an empty uploaded file. Show these controls only when
the current document has records to filter, including after Source edits. Hidden
filters must not hide ordinary prose when the last record is removed. Keep Compare
available for an empty current ontology file so deleted
baseline statements can still be reviewed. Extend the upload journeys to assert
filter visibility as well as the absence of retired requests and controls.

Remove WorkspacePlanOverview, its dedicated query/contracts and backend aggregation,
and built-in navigation to it. WorkspacePage drops unavailable restored tabs;
removed view links select the normal Workspace fallback. Review Workspace, human reviews,
approval requests and report production retain their independent owners.

Remove review-metadata/review-actions routes and feature-only DTOs, ledger parsers,
fingerprints and write stub. Shared IntentRecord ignores obsolete fields through
normal unknown-field deserialization; stored JSONL and Markdown are not migrated.
Unrelated grounding, trace and intent-history reads keep their existing APIs.

[Generate eligibility](../../ontology/features/documents.g5.md#g5.documents.generate-inputs)
continues to come from the committed docs view, managed path rules, dirty/deleted
document exclusions and existing authorization. Remove review-ledger reads and
action manifests/counts/reasons so absent, valid or malformed ledgers have no effect.
No new owner, storage, migration or lookup fallback is needed.

## Validation and rollout

Exercise empty/nonempty uploads, exact source Save/reload, Preview and Source through
Project Docs, Files and deep links, including `.semlift.md`. Assert no obsolete API
requests or controls. Check retained questions/comments/suggestions and dashboard
restore fallback. Backend checks cover unavailable retired endpoints, historical
fields ignored by intent history, and Generate unaffected by legacy ledger contents.
Run affected tests, builds, lint and browser journeys; record unexecuted coverage here.

Ship frontend and backend together, or frontend first: the new frontend needs neither
retired endpoint on either backend version. Already-loaded clients may need a refresh.
Self-review found no new state or concurrency protocol: all retained edits and domain
actions continue through existing authorities. Shared intent-record deserialization
now ignores historical assessment fields. The existing provenance backfill writes
only its owned fields into the original JSON object so those ignored fields survive.
Accepted positional records retain the existing provenance backfill and object
normalization; malformed history remains untouched.

Implementation removes the frontend surfaces and their queries, backend routes and
contracts, Generate ledger dependency, workbench entries and feature-only tests.
Local checks pass: 1,055 backend core tests, 391 contract tests, 212 route tests,
15 HTTP regressions, 430 affected frontend tests, full TypeScript checking, production
build, frontend lint and Rust Clippy across affected packages/targets. The focused
provenance regression also preserves obsolete fields and malformed history, backfills
accepted positional rows and does not move the current branch. HTTP tests verify empty/nonempty upload
and Save/reload byte preservation
and unchanged Generate input across missing, valid and malformed ledgers. Removed
grounding endpoints return 404; the removed dashboard URL follows `/docs/:docId`
and rejects its non-UUID segment with 400. The 22 center-URL tests pass, including
retired links and retained content navigation. Personal-workspace regressions also
cover the removed-view fallback while retaining restricted-surface behavior.

Eleven browser component journeys pass for upload/edit/save and retained review
suggestions. The real local API, worker and Chromium journey passes for empty,
nonempty CRLF/Unicode and legacy `.semlift.md` uploads through Project Docs, Files and
deep-link reload, asserting exact saved bytes and no obsolete requests. Retained
workbench phase/navigation and current-ontology editing/reference/comment journeys
also pass against that stack. Full remote CI is tracked on
[PR #856](https://github.com/G5Labs/G5-Lean/pull/856).

The continuation investigated the remaining CI failure in the unchanged Preview
shutdown-log journey. Reaping the launch shell immediately triggered SIGKILL for
its surviving application processes, racing their SIGTERM handlers. A local
reproduction lost the shutdown output in all five runs; retaining the remaining
grace preserved it in all five. The fix preserves the existing ten-second application-group
grace until its descendants exit, even if the shell exits first, and keeps the
existing forced-stop and cleanup order. This repairs the existing
[Preview output contract](../../ontology/software_architecture/workspace_vm.g5.md#g5.preview.output.live-tail)
without relaxing the browser assertion or the first-attempt CI requirement.
A hosted-terminal regression fails before the fix and passes afterward, covering
a shell that exits before its child's shutdown handler finishes. A second regression
checks that a surviving child which ignores SIGTERM is killed at the grace deadline.
All 28 Preview tests, both real-dispatcher output/terminal integration tests and the
Rust formatting check pass. Full remote validation remains tracked on the PR.

The empty-document toolbar correction passes 185 affected frontend tests and all
three upload component browser cases, with TypeScript, lint and the production
build also passing. The real API/worker/Chromium upload journey passes against the
rebuilt managed preview: filters stay absent for the empty file and ordinary text
typed into it through Project Docs, Files and deep-link reload. Structured and
legacy documents retain their filters; saved content remains byte-for-byte intact.
