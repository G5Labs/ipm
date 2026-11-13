# Needs Your Attention order — issue #664

Request: [issue #664](https://github.com/G5Labs/G5-Lean/issues/664), requirements R1–R5 in the
Project Docs page `docs/b66a2daf-1314-439d-b6d1-c1f8c6bacc8c/open-questions.md` (docs
repository). The owner answered all twelve questions in the set `home-attention-sort.g5q.json`
beside it on 2026-10-20 (each `decision: true`) and reviewed the clickable mocks in `mocks/`
there. Those answers are the intent source; no separate chat intent is recorded.

## Ontology changes

| ID | Change |
|---|---|
| [`g5.ui.home-attention-order`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order) | New. Order by the row's "Attention updated" time, Newest by default; the order spans all outstanding items across pages; stable ties. |
| [`g5.ui.home-attention-order-reverse`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-reverse) | New (agent). Oldest is exactly Newest reversed, ties included. |
| [`g5.ui.home-attention-order-cursor`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-cursor) | New (agent). A page position never crosses orders. |
| [`g5.ui.home-attention-order-api-default`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-api-default) | New (agent). A read naming no order stays Oldest first. |
| [`g5.ui.home-attention-order-control`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-control) | New. The Workspaces pill dropdown, "Newest"/"Oldest" only, both layouts. |
| [`g5.ui.home-attention-order-url`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-url) | New. The order lives in the URL. |
| [`g5.ui.home-attention-order-url-default`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-url-default) | New (agent). The URL omits Newest. |
| [`g5.ui.home-attention-order-filter`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-filter) | New. The filter keeps the order. |
| [`g5.ui.home-attention-order-reset`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-reset) | New. Switching returns to the first page. |
| [`g5.ui.home-attention-order-scope`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-scope) | New. Home only; the notification panel keeps its order. |
| [`g5.ui.home-attention-order-agents`](../../ontology/features/ui.g5.md#g5.ui.home-attention-order-agents) | New. No agent command until requested. |

Governing and unchanged: [`g5.ui.home-attention`](../../ontology/features/ui.g5.md#g5.ui.home-attention)
(audience, honest page counts, loading and errors never read as "nothing outstanding"),
[`g5.ui.home-paging`](../../ontology/features/ui.g5.md#g5.ui.home-paging) (the attention tile passes
the person's page size to the server), [`g5.ui.home-workspaces`](../../ontology/features/ui.g5.md#g5.ui.home-workspaces)
(the sort control being matched, its URL convention), [`g5.ui.accessibility`](../../ontology/features/ui.g5.md#g5.ui.accessibility),
[`g5.ui.api-parity`](../../ontology/features/ui.g5.md#g5.ui.api-parity) and
[`g5.design.minimal-mechanism`](../../ontology/concepts/system_design_principles.g5.md#g5.design.minimal-mechanism).
The HomeAttention model cited by `g5.ui.home-attention` checks audience and page counts, not
order; its claim is unaffected and not rerun.

## Design

### Where ordering happens (the consequential choice)

Q6 requires page one under Oldest to show the oldest outstanding items anywhere, and Show more
to continue the same order. The browser never holds the whole list: the server pages it with a
keyset cursor over a merged, bounded stream.

| Option | Verdict |
|---|---|
| **A. Server orders in either direction** — one `order` parameter on the existing read; the same total key, walked forward or backward by every source and by the merge. | **Chosen.** Keeps the bounded candidate reads and the existing cursor protocol; no new state. |
| B. Browser loads every page and sorts | Rejected: unbounded reads on Home, violates the server page size of `g5.ui.home-paging`, and the first paint would wait for the whole list. |
| C. Browser reverses loaded rows only | Rejected: this is Q6's "loaded rows only", which the owner declined. |

### Server (`g5-api-core` Home projection, owner: `home_service.rs`)

- **Operation.** `GET /orgs/:orgId/home/attention?limit=&kind=&cursor=&order=newest|oldest`. An
  unknown `order` is a 400 like an unknown `kind`. The response shape is unchanged.
- **One key, two directions.** The existing total key `(actionable_at, kind_rank, workspace_id,
  id)` stays. `oldest` walks it ascending exactly as today; `newest` walks it descending. Q3 left
  ties open, so reversing the whole key is the simplest stable choice (ties: open questions
  before approvals, then reverse workspace and id order under Newest). One direction type in
  `home_service.rs` owns "is this key beyond the cursor in this order" and the comparison; the
  sources call it instead of comparing tuples themselves.
- **Cursor.** The cursor records its order. A cursor used with the other order is rejected as an
  invalid cursor (422), as a kind mismatch already is; the UI never sends one because switching
  restarts at the first page. A cursor without the field (issued before this change) reads as
  `oldest`, so it keeps working; the version stays 1.
- **Absent `order` means `oldest`.** The API keeps its current behavior for any caller that does
  not name an order, including a browser tab loaded before deploy that is mid-way through Show
  more. The product default (Newest) is the UI's: it always sends `order`. Making the API default
  `newest` would make such an old tab's next page fail with a cursor mismatch.
- **Sources, each returning its first `limit + 1` candidates beyond the cursor in the given
  order** (the merge and `validate_candidate_bounds` are unchanged apart from direction):
  - Approvals (`ApprovalConfigService::actionable_for_org`): all caller-decidable candidates are
    already enumerated in memory; the bounded heap keeps the smallest keys today and keeps the
    largest under Newest (a reversed ordering), returning them in page order.
  - Open questions (`WorkspaceService::materialize_home_question_candidates`): workspaces are
    walked by `(docs_cut_at, workspace_id)` in the given direction, skipping those wholly beyond
    the cursor. Within a workspace every doc identity (`open_question:…`) sorts before every set
    identity (`open_question_set:…`), so Newest reads set sources first, then docs, each in
    descending id order, stopping at the shared budget. `DocService::open_project_doc_question_sources_bounded`
    takes the direction for its identity sort and cursor filter; its chunked reads and the
    closed-document scan are otherwise unchanged.
- **Bounds.** Read cost per page is unchanged in either direction (the same candidate limits and
  early stops); the debug candidate observations must hold for Newest too.

### Browser (owner: `pages/HomePage.tsx` for state, `components/home/HomeTileParts.tsx` for the control)

- **Same control by construction (R4).** Extract the Workspaces tile's pill `<select>` into a
  shared `HomeSortSelect` in `HomeTileParts.tsx` (label, value, options, test id, onChange) and
  use it in both tiles. The Workspaces tile's look, test id and behavior do not change. The
  attention control has the accessible name "Sort attention items" and the options "Newest"
  and "Oldest"; a native select announces its value, so screen readers hear the order (R5).
- **Placement.** In the `NeedsAttention` header actions beside the kind chips. The tile is the
  same component in both layouts, so the control appears in both (Q10).
- **URL state (Q4).** `?attentionSort=oldest`; Newest is omitted. Read with `useSearchParams` and
  written with `replace`, as the Workspaces tile does. It coexists with `workspaces` and `sort`.
  The kind filter stays in component state as today and does not touch the order (Q5).
- **Query.** `homeAttentionQueryOptions` gains the order, sends it as `order` and includes it in
  the query key. **First page on switch (Q7):** an infinite query cached for the other order
  would come back with every page it had revealed, so choosing an order first removes the cached
  query for the target order, then writes the URL. While the new order loads the tile shows its
  normal loading state with the new choice already selected; a failure shows the usual error and
  Retry, which reloads the chosen order (mock assumptions A1, A2).
- **Refresh cadence (System Engineering question in the requirements page).** Keep the current
  reads (15 s stale time, re-read on window focus and on returning to Home once stale). Newest
  makes fresh arrivals visible at the top on the next read; no requirement asks for live arrival,
  and a timer would rerun the approval derivation and live docs overlays for every visible
  workspace each tick. Revisit only if the owner asks for it.

### Not changed

The notification panel (Q11); `g5d` and any agent command (Q12 — the backend parameter exists
because the UI needs it); the Workspaces sort's choices; the kind filter's persistence; the
HomeAttention model.

## Phases

1. **Server order.** Direction type and cursor field in `home_service.rs`; route parameter in
   `routes/home.rs`; direction through the approval heap, the question workspace walk, the doc
   identity sort and the set ordering. Integration tests below.
2. **Browser.** `HomeSortSelect` extraction (Workspaces tile unchanged), the attention control,
   URL state, query key and the first-page reset; `HomeAttentionOrder` in `lib/types.ts`.
   Component tests below.
3. **Browser flow and ontology metadata.** Extend the alignment Home tiles flow; update the
   two new blocks' metadata with implementation locations and evidence; report the run.

## Acceptance outcomes

| Outcome | Evidence to produce |
|---|---|
| R1/Q1: Home opens on Newest, the newest "Attention updated" row on top, for everyone. | Page test: default request carries `order=newest`, control shows Newest, URL has no `attentionSort`. |
| R2/Q2: Oldest is the exact reverse of Newest over the whole list; ties stable. | `api_home::` integration: walking all pages under each order at limits 1, 2 and 50, with mixed kinds and tied open-question times, yields one list and its exact reverse, no duplicates or skips; repeated walks are identical. |
| R3/Q6: each Show more page continues the order. | Same integration walks; candidate observations stay within `limit + 1` per stream under Newest; page test that Show more requests the next cursor with the same order. |
| Q7: switching returns to page one, including back to a previously revealed order. | Page test: Show more, switch, switch back → only the first page of rows and a first-page request. |
| Q4: reload or link keeps the order; arrival from navigation shows Newest. | Page tests: arriving with `?attentionSort=oldest` requests Oldest; the Workspaces `sort` param is independent. |
| Q5: the kind filter keeps the order. | Page test: change kind under Oldest → request carries `kind` and `order=oldest`. Integration: kind-filtered walks in both orders. |
| R4/Q8/Q10: same control and look as the Workspaces sort, both layouts. | Both tiles render `HomeSortSelect`; page tests find the attention control under alignment and classic layouts; existing Workspaces sort tests pass unchanged; browser flow sees both pills. |
| R5/Q9: the control shows only "Newest" or "Oldest" and is named for screen readers. | Page test by role and accessible name "Sort attention items" with the selected option text. |
| Cursor safety and compatibility. | Integration: a Newest cursor under `order=oldest` (and the reverse) → 422; an unknown `order` → 400; no `order` equals `oldest`; a cursor without the order field continues oldest. Existing `api_home` tests pass unchanged. |
| `g5.ui.home-attention` still holds. | Existing loading, error, retry and page-count tests pass with the new parameter. |

Checks: `cargo fmt --check`, strict clippy and the `api_home::` selector of the single g5-api
`integration` target plus the touched crates' unit tests; frontend typecheck, lint and Vitest for
`HomePage.test.tsx` and the Home tile tests; the alignment Home tiles Playwright flow where the
VM can run it (otherwise reported as unexecuted).

## Risks

- **Moving times across pages.** Keyset paging over a live projection already lets an item whose
  time changes between pages appear twice under Oldest or be skipped until the next read under
  Newest (the HomeAttention model excludes cross-page consistency). This change does not widen
  it; a refresh re-reads every revealed page from the first cursor. Not fixed here.
- **Descending question walk.** Getting the doc/set precedence or the workspace skip wrong breaks
  the "stream is a prefix of the order" property silently. Mitigation: the integration walks
  compare full Newest and Oldest lists at several page sizes, with docs and sets in one workspace.
- **Shared control refactor.** Extracting the select could change the Workspaces tile. Mitigation:
  keep its test id and classes; its existing tests must pass unchanged.
- **Stale cached pages.** Without the removal step, switching back would restore revealed pages
  and break Q7; covered by the switch-back test.

## Open assumptions

- **A3 (mock): no extra announcement after a switch.** The mock announced "Showing oldest first"
  once. The design adds no live region: the select announces its new value, the tile's loading
  status is announced, and the Workspaces sort announces nothing. If the owner wants the
  announcement, it is a small addition to the control block.
- **A4 (mock): URL parameter name.** `attentionSort=oldest` replaces the mock's placeholder
  `attention=oldest`, keeping `attention` free for a future kind filter in the URL.
- **A5 (mock): requirement wording.** Resolved: at the owner's request the requirements page
  now names the choices "Newest" and "Oldest" in R1, R2 and R5, per Q9, and its status reads
  answered.
- **Tie order under Newest** is the full reverse (questions before approvals at an equal time).
  Q3 accepted any stable order.
- **API default `oldest`** when `order` is absent, for compatibility; only the Home UI calls this
  read today and it always names the order.

## Implementation record (2026-10-20)

Implemented as designed; no governing amendment was needed. Code: `home_service.rs`
(`HomeAttentionOrder`, order-carrying cursor, `is_after`/`passes_prefix`, merge),
`approval_config_service.rs` (page-ordered heap), `workspace_service.rs` (direction-aware
workspace walk and per-workspace source sequence), `doc_service.rs` (identity order),
`routes/home.rs` (`order`); `HomeTileParts.tsx` (`HomeSortSelect`), `HomeWorkspacesTile.tsx`,
`useHome.ts`, `HomePage.tsx`, `lib/types.ts`.

Discovery, routine: the debug-only Home read observations were drained process-wide, so
`api_home` tests running in parallel stole or polluted each other's records (seen as one
failure each in `…filters_before_capture…` and `…batches_approval_reads…`; both pass
alone). The new tests widened that race. Observations now carry the organization and
drain per organization. Test infrastructure only; no contract change.

Evidence, all on this branch:

- `cargo test -p g5-api --test integration -- api_home:: api_question_metrics:: api_question_sets::`:
  21/21 in three consecutive parallel runs after the observer fix, including the two new
  ordering tests. `cargo test -p g5-api-core --lib`: 1032 passed, including 3 new
  `home_service::tests`. Strict clippy (`-D warnings`, `--no-default-features`) on
  `g5-api-core`, `g5-api-routes`, `g5-api` and `cargo fmt --check` pass; a release-profile
  `cargo check` of the two production crates passes, covering the debug-gated observer fields.
- Vitest: the full suite passes (301 files, 4726 tests); `HomePage.test.tsx` has 79, with 5 new order tests. Removing the cache reset fails
  the switch-back test, so that test discriminates. Typecheck (`tsconfig.json`,
  `tsconfig.build.json`) passes; ESLint shows only pre-existing warnings.
- Browser E2E: the committed flow in `alignment-shell.spec.ts` and the existing Workspaces
  tile flow pass in Chromium against a native stack: `g5-api` built from this branch, test-token
  auth, migrated Postgres, local Git repositories, and the Vite dev server with the E2E flags.
  A local-only harness (not committed) also walked 12 real approvals through Newest, Oldest,
  Show more, switch-back, reload and the filter, and checked that every UI request names an order.
  The first CI run (build `25f1f178`) failed this flow on every attempt: it compared the tile
  with its own API walks while other workers fork, edit and delete forks of the shared target,
  each fork carrying the flow's approvals. The flow now compares the tile with the responses it
  rendered and checks the rows are in the chosen order; `api_home.rs` keeps the server order.

Gaps: the CI Docker/VM E2E lane did not run here (no KVM, no images), so the full
`alignment-shell` suite is left to CI. No Verus setup exists, so no production proof. The
open assumption A3 above still stands.
