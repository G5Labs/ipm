# Whitespace review placement and sidebar height

Implements [#574](https://github.com/G5Labs/G5-Lean/issues/574) under
[review placement](../../ontology/features/documents.g5.md#g5.documents.review-placement),
[acceptance](../../ontology/features/workspaces.g5.md#g5.workspace.review-acceptance)
and [sidebar layout](../../ontology/features/ui.g5.md#g5.ui.review-sidebar).
The authorized checkout branch is `fix-suggestions-render` in both the code and
project-docs repositories.

The existing projection already verifies raw source identity and the server owns
acceptance. Keep that owner and its fresh live-file checks. Separate verified raw
ranges from presentation: use the rich preview where faithful, otherwise a local
source diff with visible spaces, tabs and line endings. Retain exact source text
in the projection, never serialize the rendered markers into an edit. Represent
separator and file-edge placements as boundaries between canonical blocks; do not
invent an original block identity for an empty row. Preserve structured question
controls and semantic record frames.
Whitespace-only edits inside a record label or question use a source diff beside
the canonical frame, so the source location and existing controls both survive.
Other structured changes retain their existing conflict presentation.

Source inspection also found two acceptance dependencies: hunk capture adds a
synthetic final newline, and pure insertion matching trusts repeated context at
an old line. Capture actual EOF termination for changed hunks in both producers
(review rounds and FILES pushes), and splice verified raw ranges for durable
edits. Keep the existing exclusion of final-newline-only publications and the
legacy line fallback for old normalized captures. For untrusted pure insertions,
both projection and acceptance require a single matching context at the recorded
boundary; a shifted or repeated frame uses the conflict view. Link destination
changes continue to keep canonical navigation and comment controls through the
existing conflict presentation.
Recovery recognizes whole-file and context-framed application only when the
proposed range's raw bytes match, including EOF termination; equal normalized
lines alone cannot settle an edit.

Missing or ambiguous placement uses a conflict view with original/current/suggested
source. No new manual relocation API or guessed anchor is needed: the existing
server matching rules remain authoritative. Retain each durable edit's existing
Accept/Edit/Reject/Discussion card and independently selected outcomes. Competing
overlapping proposals stay separate and unresolved.

`SideThreads` owns scrolling for every shared sidebar consumer. Its suggestion
list has a sidebar mode without its own height limit or scrollbar; the existing
below-editor mode keeps its cap.

Validate source projection and actual editor/card behavior for leading/trailing
spaces, blank additions/deletions, empty source, file edges, changed context,
duplicate/missing matches and competing proposals. Browser layout checks must
exercise full-height sidebar scrolling and final-card controls. Reuse and extend
the real Push review/selected-decision journey to verify exact persisted bytes
and both entry points. Run affected tests, build and lint after final changes;
record any unavailable system coverage here.

## Verification

- 328 affected frontend tests pass, including source placement, canonical
  comments, drafts, FILES consumers and hidden-content handling.
- Five Chromium component journeys pass with the actual editor and sidebar:
  whitespace placement and adjacent cards, literal edit payloads, discussion,
  rejection failures retaining drafts, full-height scrolling, final-card access
  and the independent below-editor cap. Run with
  `cd frontend && npx playwright test -c e2e/browser/playwright.config.ts`.
  These use HTTP doubles; they do not establish backend persistence.
- 24 review-service and 11 edit-ledger unit tests pass. The API boundary test
  publishes real Git diffs, accepts and reloads 12 whitespace variants, including
  an owner-edited replacement and absent final newlines. The native fixture and
  write/checkpoint failure API regressions also pass. Missing/duplicate source
  checks refuse writes, keep edits open, and preserve discussion and rejection.
  Recovery also refuses normalized-only EOF matches while accepting exact prior
  application; the real API regression failed before the receipt fix and passes
  after it. Unit cases cover whole-file and context-framed recovery in both
  final-newline directions.
  These use PostgreSQL and
  the application HTTP routes, with a scripted guest runtime peer; source Git
  checkpoints are explicitly seeded by the existing test harness.
- TypeScript, changed-file ESLint (existing warnings only), Prettier and Rust
  formatting pass. Local Vite production bundling exhausts memory during
  transformation, including with a larger heap and Node 24. Production bundling
  and the existing full-stack Playwright flow remain CI checks; the browser
  fixture and scripted guest do not replace a real workspace-VM journey.

## PR shipping follow-up

CI on `edbbd9b` passed the Rust lane but failed E2E's `tsc -b` check on the new
Testing Library assertion's unsupported `exact` option. The production-only
local typecheck had excluded that test; validate with CI's full typecheck too.
CodeRabbit's EOF recovery finding is in scope: follow the acceptance route's
`whole_file_parent` and context-framed paths and require exact receipt bytes.
The browser working-directory finding is checked through a fresh server startup
and npm's actual parent-package resolution, rather than inferred from the
configuration directory alone. Keep the sidebar metadata entry in body order.

The full CI typecheck (`npx tsc -b`), 88 renderer tests, a fresh-server run of all
five browser checks, all 24 review-service unit tests and all three whitespace
API journeys pass after these fixes. Prettier, Rust formatting and diff checks
also pass. CodeRabbit accepted the clean-start evidence and withdrew its browser
working-directory finding; the existing launcher remains unchanged.
