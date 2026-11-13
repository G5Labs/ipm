# Question store review

Self-review by the implementing agent (not independent) of branch `g5/question-store-plan` against [the plan](ontology-plan-question-store.md), its `Ontology changes and governance` inventory and the blocks it cites. Comparison base `origin/main` `d5b94253`.

## Scope reviewed

The changed blocks (`g5.story.answer-a-question-anywhere`, `g5.documents.question.referenced`, `.writer`, `.answer-checkpoint`, `.decision`, `.readers`, `.agent-access`, `.html`, `.metrics`, `g5.architecture.question_owner`) and the governing rows of the plan's table, against the actual diff: contract `backend/g5-contracts/src/types/question_set.rs`; `services/workspace/workspace_file_save.rs` and `question_set_service.rs`; `doc_service.rs`, `workspace_service.rs`, `home_service.rs`; `routes/questions.rs`, `routes/internal.rs`, `routes/workspaces.rs`, `auth_middleware.rs`; `backend/g5d/src/cli/question.rs`; the frontend library, hook, cards, bridge, frame, explainer and status-band changes; `.agents/AGENTS.md` and the two chat prompts; the fake provider workflow and the tests.

## Initial findings (before repairs)

| # | Governing rule | Path | Finding | Fix | Recheck |
| --- | --- | --- | --- | --- | --- |
| 1 | `g5.design.minimal-mechanism` (lint gate) | `doc_service.rs` test, `g5d/src/cli/question.rs` | Clippy `useless_format`, unused import, `cmp_owned` with `-D warnings`. | Fixed. | clippy clean on touched crates. |
| 2 | ontology format | `documents.g5.md` | The plan's ontology edit duplicated the `Answering is an explicit save of one file` heading and the `g5.story.trace-decisions-to-implementation` anchor. | Removed both duplicates. | Inspection. |
| 3 | `g5.architecture.commands` (layering inventory) | `backend/g5-api/tests/service_context_layering.rs` | The two new workspace modules had no reviewed home. | Added. | Test passes. |
| 4 | `g5.documents.question.writer` | `routes/workspaces.rs::rename_workspace_file_for_actor`, `workspace_reviews.rs` accept/discard writes | The plan named PUT and the doc routes; a rename onto a set path or an accepted review suggestion on a set file landed bytes unchecked. | The rename reads the moving file and runs the schema check; `write_live_workspace_file` (every route live write, review paths included) runs it. | Rename refusal added to `api_question_sets.rs` → pass; review suites pass. |

## Commitment accounting

`commitment → actual path → counterexample/check → result`

- `.referenced` → `g5-question-ref` body `{"path","id"}` (`questionSet.ts::parseQuestionRef`, `doc_service.rs::parse_question_ref_body`); placeholder `data-g5-question="<path>#<id>"`; `parse_question_set_path` confines to `docs/<one segment>/questions/<set>.g5q.json` → traversal/extra-segment/backslash/other-extension cases in contract, TS and API tests (`unparseable_sets_…` path loop) → pass. Fork inheritance → `a_fork_resolves_and_answers_an_inherited_set_independently` (child reads and answers the parent path; parent unchanged; child's own dir excludes it) → pass. FILES shows the fence as code → `MarkdownPreview.test.tsx` → pass. SpecEditor and explainer render cards → unit tests + browser flow (see Runtime evidence).
- `.writer` → `QuestionSetService` is the only writer (public and incarnation adapters call it); provenance from `QuestionActor`; body `answeredBy`/`provenance` → 422 (unit + API, both routes); file PUT refuses unparseable and renamed-id sets (API); replay requires key+actor+operation+payload (contract `replay_requires_…`, API key reuse → 409 `idempotency-key-reused`) → pass. No signature, classifier or repair exists.
- `.answer-checkpoint` → `save_live_file` (write under `expectedHash` + VM, identical-bytes retry, `checkpoint_selected_files` with the observed blob) shared by the editor Save and the service → API asserts one commit and one push invoke per person answer, none for agent writes; push denied → `notPublished`, lost-response replay reports the applied hash, wrong hash → `reload-required`, exact hash after the push recovers → `published` without a second revision, a changed set before retry → `reload-required`; stopped workspace → `WORKSPACE_NOT_RUNNING` and no `workspace_vms` row → pass.
- `.decision` → `is_decision` = answered ∧ user; agent answers forced `proposed` by the shape gate; agent cannot replace/clear a person's decision (`foreign-answer-immutable`, API); wording edit keeps the answer with `answeredAtRevision` < `revision`, option/multi-select changes on a decision → `needs-new-question` (contract) ; UI shows "answered against revision N" (card unit test) → pass.
- `.readers` → reader GET serves `committed`, `answerable: false`; reader POST → 403; zero VM RPCs from reader requests (API) → pass. No share links.
- `.agent-access` → `g5d question list|create|answer` (CLI unit tests), internal routes on the allowlist (middleware unit test), live-incarnation + `DirectAgentWrite`, agent provenance, live-only writes, replay key `agent:<set>:<question>:<rev>`, revision-moved prints the current entry; another workspace's token → 403 (API) → pass. Guidance in `.agents/AGENTS.md` and both prompts (prompt test).
- `.html` → host validation (`validateQuestionProposal`: frame source, generation, listed reference, size, current revision, offered options, answerable) unit-tested per rejection; renderer builds DOM with `textContent`, keeps unsent input across same-revision updates, re-renders on new revision, only posts a proposal (renderer test); confirmation in `QuestionAnswerBar`; data posted by message so `srcDoc` does not change.
- `.metrics` → third family in `open_question_metrics` and Home; counted once with two referencing docs; inherited referenced set counted; unreferenced sibling set not counted; invalid set flagged `questionSetsIncomplete`; embedded family unchanged; Home `open_question_set:` items link a referencing doc (`api_question_metrics`) → pass. Refresh: `question-set` queries join the docs-repository `workspace.file_changed` family and the durable-cut invalidation; no new event.
- `g5.architecture.question_owner` → composition only (DocService reads, WorkspaceRuntime writes, workspace_file_save checkpoint); no table/key/cache/event; route code no longer owns write-then-checkpoint (`current_reader_contract` pins `save_live_file` in the editor Save) → pass.
- Preserved: `g5.documents.editor.question` (embedded fence untouched; existing suites pass), `g5.documents.files-view.preview`, `g5.git.checkpoint.*`, `g5.documents.files.version`, `g5.security.admission`, `g5.lifecycle.start.consent`.

## Runtime evidence

All on the review host (macOS, Docker Desktop), this checkout:

- `SQLX_OFFLINE=true cargo check --workspace --all-targets --offline` → ok.
- `cargo clippy -p g5-contracts -p g5-api-core -p g5-api-routes -p g5d -p fake-provider-cli --no-default-features --features g5d/test-helpers --all-targets --no-deps -- -D warnings` (with `RUSTFLAGS=""`) → ok after fixes.
- Lib tests: `g5-contracts` 393, `g5-api-core` 1033, `g5-api-routes` 228 passed; `g5d --no-default-features --features test-helpers` 225 passed, 1 failed (`cli::preview_serve::tests::a_daemon_started_under_setsid_outlives_the_app_stop_for_cleanup`, an untouched module; it fails alone on this host); `fake-provider-cli` 43 + 44 passed.
- API tests against Postgres 17 in Docker: `api_question_sets` 6/6, `api_question_metrics` 1/1, `api_home` 12/12, `api_workspace_files_live_tree` 6/6, `api_workspace_fork_owner_access` 6/6, `current_reader_contract`, `route_auth_static`, `route_sql_guard`, `service_context_layering` pass; `api_workspace_reviews` 59/61 under 4 threads, the two failures (`start_review` under load) pass when rerun alone.
- Frontend: `tsc -p tsconfig.build.json` and `npm run build` ok; eslint has no errors on touched files; vitest 4407/4408, the one failure is the known `ChatPanel` account flake (fails alone too, untouched file).
- **Not executed:** `frontend/e2e/flows/question-store.spec.ts`. `just ci-docker-prepare` stops at Phase 4b: "requires /dev/kvm; workspace-VM validation has no process fallback" (Docker Desktop on macOS); the native libkrun host cannot load `libvirglrenderer.1.dylib`. The local preflight also fails on an unrelated base violation (`tests/e2e/src/scenarios/ci_e2e_lane_isolation.rs` literal container names). The flows it would prove — real VM commit/push of a card answer, stale-tab refusal in the UI, stopped-workspace UI, explainer host confirmation against forged frame messages, and the agent turn through `g5d question` — are covered at the API/unit level only.

## CI follow-up (PR #409, 2026-10-09)

Driving `g5-ci-rust` and `g5-ci-e2e` green after this review:

- `rust-fmt`: one formatting diff in `api_question_sets.rs`.
- `authoritative_workspace_reader_guard::all_backend_provider_protocol_uses_its_owner`: `g5d question` had copied `g5d issue`'s raw `reqwest` client. The plan named the incarnation client as the transport, so `InternalApiClient` gained `list_question_sets`, `create_question_set` and `answer_question`, and maps a 409 `QUESTION_CONFLICT` envelope to `G5Error::QuestionConflict { details }`; the command keeps printing the current entry on a moved revision. Two independent Codex reviews approved.
- `g5-ci-e2e` run 1 failed before any browser test: `scripts/ensure-base-images.sh` could not initialise base-image lock ownership while three seed steps started together. The same error hit PR #382's build in the same hour and this PR does not touch the script; a re-run passed. Pre-existing race, not investigated further here.
- `question-store.spec.ts` ran for the first time and needed four corrections, all in the spec: the stale tab is an unrefreshed tab, so the set read is pinned before the other tab's answer instead of reloading; a stopped workspace lists only the committed cut, so the Project Doc is committed through the workspace files route (`expectedVmId`, `commitAndPush`) before the stop; every listed doc carries a path-derived overlay id, so the committed state is the entry's `source`, not its id shape; a missing card now fails with the page's URL, test ids and text.
- One product defect found by the flow (`g5.workbench.landing`: a doc/file path wins): a deep link whose tab was restored as the persisted active tab never ran `openTab`, so Chat stayed in the center over the selected document on every reload of a document URL. `WorkspacePage` now shows the route's content tab even when it is already selected (`showRouteTab`), covered by `WorkspacePageAlignment.test.tsx` ("shows a deep-linked doc that is already the persisted active tab"), which reproduced the defect before the fix.
- Unrelated flake seen once in the lane: `native-requests.spec.ts` org invite returned 500 and passed on retry.

## Remaining gaps

| Governing ontology | Gap | Proposed action | Uncertainty / alternatives / blockers |
| --- | --- | --- | --- |
| `g5.documents.question.metrics` | Refresh is filtered by repository, not path: any docs-repo change re-reads every open set query. | Accept; path filtering needs the coalescer to keep paths. | Cost only, no correctness gap. |
| `g5.documents.question.metrics` | HTML explainers' placeholders are not scanned for `referencedBy`, so an explainer-only set is counted but has no Home item. | Scan HTML Project Docs for placeholders in the same corpus read. | Small follow-up. |
| `g5.documents.question.metrics` | Home runs the full corpus scan beside the bounded one for each workspace with budget left. | Fold references into the bounded scan. | Performance only. |
| `g5.documents.question.referenced` | SpecEditor's record accent and outline open-count ignore referenced entries (the record frame uses its label tags). | Resolve referenced entries in `SpecEditor` as G5 PR #2532 did. | UI polish; metrics count correctly. |
| `g5.documents.question.referenced` | Markdown explainers rendered through `MarkdownRenderer` show the reference fence as code, not a card. | Add the card to `MarkdownRenderer` for managed explainer paths. | Plan scope names managed explainer views; HTML is covered. |
| `g5.documents.question.decision` | After an ownership transfer the new owner cannot change the previous owner's decision (`foreign-answer-immutable`); they can only add a new question. | Human decision: keep (decisions are a person's record) or allow the current owner to supersede. | Inherited from G5 PR #2532's gate. |
| all question blocks | The browser flow runs only in `g5-ci-e2e` (no KVM on the review host). | Keep it in the lane; see the CI follow-up above for the corrections its first runs needed. | Local reproduction stays unavailable on macOS. |
| `g5.documents.question.agent-access` | No separate Rust E2E scenario (`tests/e2e/src/scenarios/question_store.rs`); the agent path is the browser flow's third test plus API tests. | Add a scenario if the browser lane is too slow. | — |

## Votes amendment review (2026-10-09, branch `feat/question-vote-page`)

Self-review by the implementing agent of the [votes amendment](ontology-plan-question-store.md#amendment-votes-2026-10-09) against `g5.documents.question.votes`, the blocks it touches (`.readers`, `.html`, the story) and the governing rows of the amendment table; comparison base `origin/main` `9efa4c27`. Two independent Codex reviews (general correctness and security; duplication and reuse) ran on the commit before push; their findings and fixes are recorded below.

### Independent review findings (two Codex reviews on commit `282a8e38`, before push)

| # | Severity | Finding | Fix |
| --- | --- | --- | --- |
| 1 | major | A vote carried no revision: a question reworded while the host-bar confirmation was pending was stamped with the new revision. | `VoteCommand.expected_revision` (optional on the API, always sent by the cards); a moved revision is refused with `QUESTION_CONFLICT revision-moved` and the current entry, which the cards install like an answer refusal. API, bridge and card tests. |
| 2 | major | The explainer card seeded a reader's inputs from their vote, so an answered question showed the reader's option selected and never named the decided option. | The in-frame summary now lists the decided (or proposed) option labels (`data-g5-question-decision`); renderer test with a conflicting vote. |
| 3 | major | `publish.sh` accepted an existing set on the owner's live read, which can be uncommitted. | An existing valid set is re-published through the files route (same bytes, `commitAndPush`) before the page. |
| 4 | minor | `question.vote` used plain invalidation, which can miss an event during a fetch; the durable-cut refresh omitted votes. | `invalidateAndReadAgain` for the event; votes re-read with the durable cut. |
| 5 | minor | A failed tally read rendered as "No votes yet". | Both cards show loading and unavailable states; the Vote control waits; zero counts come only from a successful read. |
| 6 | minor | The API "reader" held the write scope; no `/unvote` denial; no invite-only case. | Readers hold the read scope only; stranger and uninvited member are refused on all three routes; an invited member votes and withdraws. The second-reader browser flow stays a gap (OAuth stub). |
| 7 | minor | Duplicated identity lookup, workspace resolution, row conversion and tally formatting. | `UserService::load_summaries` shared with comments, `resolve_org_id_for_workspace`, `VoteRow::into_dto`, `optionVoterNames`/`optionTallyLabel`, `questionVotesQueryKey` in `ws.ts`, `WithdrawVoteOutcome` naming. |

### Commitment accounting

- `.votes` → `question_votes` rows under the tenant RLS shape (`075_question_votes.sql`); `QuestionVoteService::vote` validates against the set the voter resolves (`QuestionSetService::get_set` with the voter as viewer: live for the attached owner, committed for every reader), refuses an invalid set, a missing question, an unlisted option, a second option on a single-select question, over-long or empty votes, and dedupes option ids; `withdraw` is a no-op for an absent vote; `list_votes` joins display names from `users` on the system pool. Routes `GET /votes`, `POST /vote`, `POST /unvote` sit under `Scope::WorkspacesRead` with the same static-audit exception as `comments.rs::add_reply`; bodies never name the voter (`voter-not-accepted`); the secondary-identifier audit passes because handlers convert bodies to service commands before use. → `api_question_votes.rs`: owner and reader vote on a committed set with no `workspace_vms` row; `question.vote` reaches a reader's workspace channel; change, withdraw and repeat withdraw; every refusal code; an unknown set is 404; the attached owner votes on a live-only option while a reader of the committed cut cannot and no reader request reaches the VM; a stranger from another organization is refused on both routes and writes nothing; a fork lists no votes at the inherited path and its votes stay out of the parent → pass.
- A vote never touches the set file or the answer → the vote service holds no file write; `api_question_votes.rs` reads the set after votes (`status: open`, `decision: false`); the browser flow asserts the set file stays published after a vote.
- `.html` (host confirmation for votes) → the renderer posts `propose-question-vote` (with `withdraw`) and `validateQuestionProposal` accepts both message kinds under the same frame, generation, reference, size, revision and option checks, dropping an answer from a non-answerable set and any proposal on an invalid set; `QuestionAnswerBar` confirms with Vote / Withdraw vote. → `questionCardBridge.test.ts` (vote and withdrawal accepted on a read-only set; unknown option, stale revision, empty vote and invalid set dropped), `questionCardRenderer.test.ts` (tally rendered as text, viewer's vote seeds the selection, Vote and Withdraw propose only, inert when not votable), the browser flow (dismissed proposal records nothing; confirmed vote shows `(you)` under the option).
- Cards show the tally to everyone → `ReferencedQuestionCard.tsx` (`OptionTally`, vote count, free-text votes, Vote / Change vote / Withdraw vote for every reader; the owner's Answer controls unchanged) and the in-frame renderer; `useQuestionVoteWrite` replaces the viewer's vote in the cached view and re-reads; `lib/ws.ts` re-reads the set's votes on `question.vote`. → `ReferencedQuestionCard.test.tsx` (tally with names and `(you)`, change and withdraw through the routes, refused vote explained), `questionSet.test.ts` (tally counts each voter once, per offered option, marks the viewer).
- `.readers`, `.decision`, `.answer-checkpoint` preserved → reader answers are still refused (`api_question_sets.rs` unchanged and passing); `is_decision` and the answer gates are untouched.
- `g5.design.minimal-mechanism` → one table, one service, no cache, no maintenance loop; the event reuses the workspace channel. The vote route opens the workspace-scoped connection twice (once to capture the docs cut before Git I/O, once for the row write), which keeps the question routes' rule of closing the transaction before repository I/O at the cost of a second admission check.

### Runtime evidence

On the review host (macOS, Docker Desktop Postgres 17), this checkout: `cargo clippy -p g5-api-core -p g5-api-routes --all-targets -- -D warnings` clean; `g5-api-core` lib 1031 passed; `g5-api-routes` question unit tests; API suites `api_question_votes` 3/3, `api_question_sets` 6/6, `route_auth_static` 12/12, `route_sql_guard`, `current_reader_contract`, `service_context_layering` pass; frontend `tsc` clean, eslint no errors on touched files (pre-existing warnings only), the related vitest folders 848/848 and the whole suite (see the PR); the vote page renders in Chromium (light and dark) without script errors and every placeholder names a question in `questions.json`, whose option ids match the mock keys. **Not executed:** the new browser flow in `question-store.spec.ts` (no `/dev/kvm` on this host; the `g5-ci-e2e` lane runs it) and `publish.sh` against a live workspace.

### Remaining gaps

| Governing ontology | Gap | Proposed action |
| --- | --- | --- |
| `g5.documents.question.votes` | `g5d question list` prints no tally; agents cannot read votes. | Add a `votes` column once agents need it; the ontology says agents act on decisions. |
| `g5.documents.question.votes` | Votes are keyed by question id, so a vote cast against an earlier wording stays counted after a revision (the row records `question_revision` but no card shows it). | Show "voted against revision N" like the answer does, or clear votes on a wording edit (human choice). |
| `g5.documents.question.votes` | The browser flow votes as the owner only; a second member's vote through the OAuth stub is covered at the API level. | Extend the flow with `useNonOwnerMemberIdentity` when the stub lane allows a second session. |
| `g5.documents.question.metrics` | Metrics and Home ignore votes (no "waiting for votes" state). | None planned. |

