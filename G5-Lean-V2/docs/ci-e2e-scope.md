# Current G5-Lean E2E scope

CI validates the current Lean implementation. It does not require completing the dispatcher redesign or restoring deleted worker commands. Both PR checks must pass on the exact candidate before merge.

## Required coverage

- Native Git publication: authenticated smart HTTP, assigned job/task ownership, workspace pushes, stale-ref rejection, peer advances, and fresh-clone readback.
- Real guest chat edits, the large-repository publication bound, ordinary job completion, budget exhaustion/retry, provider interruption, and worker loss.
- Live approval-review generation and settlement. Automatic worker completion reports every assigned repository after native pushes, including unchanged members, using the same per-repository contract as CLI completion.
- Current Sync without ontology dependencies, autonomous continuation bounds, and AQL CLI admission and ordinary task lifecycle.
- Browser authentication, downloaded-worker Claude/Codex chat and follow-up, provider routing/settings, human questions, document editing and reference resolution, organization isolation, and coordinated base pull.
- Multi-repository inventory, admission, snapshot, isolation, and native-push contracts remain in the API and Rust E2E suites.

Each required test has one execution owner. The value-loop receipt lane runs explicit formal reviews (RV7) and API-restart continuity (M6), and the broad Rust lane skips exactly those tests ([single execution owner](../.g5/ontology/features/operations.g5.md#g5.quality.delivery.ci.single-execution-owner)). Browser files are placed on the two shards from measured durations ([browser shards](../.g5/ontology/features/operations.g5.md#g5.quality.delivery.ci.browser-shards)).

The shared view-scope catalog generates both seeded document references and SQL links. Its production-parser regression preserves all twelve references per view. Code and docs mirrors stay private until fixture SQL identities and links are ready, preventing background projection from racing the seed. Seed and test failures remain failures; the final CI gate requires all stage receipts. Rust test lanes continue after a failed lane to expose subsequent failures, while retaining a failed final exit status. Compilation failures still stop before tests.

Chat publication reads authoritative committed snapshots, starts from the confirmed seed commit, and requires exactly the three requested commits with their expected subjects. The shared materialized-branch fixture uses that same authority for Sync and its sibling scenarios. Cached branch-detail SQL heads are not Git authority. Clone checks retain real commit/file readback even when SQL incorrectly labels an existing Git ref unborn or caches a nonexistent SHA. Actual absent refs still fail closed in the API authority tests. New editor drafts and their commits use the exact Git head after native publication.

Sync admission retries at most twice, only for an HTTP 503 carrying the typed `DATABASE_RETRYABLE` code. SQL deadlock/serialization victims roll back, and provisional job forks are removed at their original heads before retry. Fault-injection tests require exactly one job/task after recovery and no job when retries are exhausted. Transport failures, unrelated server errors, and successful requests are never replayed. PostgreSQL logs are retained to identify the still-unresolved conflicting query pair. Projection waits remain bounded and verify the exact required branch/commit entries; P8.9 removed the search projection, leaving head_cache, docs and ontology.

Budget resumption checks both checkpoint and resumed content through fresh authenticated Git clones, including commit ancestry. The former assertion that native workers create/preserve CRDT draft sessions or update cached SQL heads is retired. Task attempts, provider session identity, checkpoint bytes, commit ancestry, and exactly two microVM launches remain required. Selected-task output and diff reads remain required too: native tasks without CRDT sessions read their assigned Git cut. Existing task drafts must still form a complete matching overlay; partial drafts and sibling state never justify a fallback to committed content.

## Retired legacy combined-flow tests

These deterministic scenarios required fake Node-sidecar workflows whose `g5d commit`, `g5d task rebase`, and worker CRDT publication actions were deleted. The replacement fake CLI explicitly refuses these workflows. They are removed rather than skipped or reported as passing:

| Former scenario | Retired expectation |
| --- | --- |
| `live_host_conflict_merge_turn_runs_agent_in_guest` | Managed task rebase, CRDT reseeding, and checkpointed conflict-merge reports |
| `live_host_autonomous_sync_resolves_ambiguity_without_human_round_trip` | The legacy fake AQL planner/child registry and its combined decision-document/automatic-merge flow |
| `multi_code_doc_sync_value_loop` and `multi_code_chat_value_loop` | The removed multi-repository fake workflow's connected commits and run-owned merge report protocol |
| `frontend/e2e/flows/value-loop-matrix.spec.ts` | A single combined chat/New Doc/Upload → legacy AQL wait modes → source-only rebase → PR-creation acceptance flow |
| `committed_docs_seed_push_preserves_failed_projection_staleness` | Cross-workspace push implicitly adding repositories, then treating projection failure as Git publication failure; native push does neither. Explicit connected-document clean-marker and native push/readback tests remain. |
| `e2e_provider_adapters_derive_openable_canonical_citations` | Worker-recorded canonical `resolvedPaths` from the deleted Node-sidecar `tool_path_projection` module. API tests for persisted provenance, legacy citation rows, and exact repo/path resolution remain; current worker adapters do not supply that projection. |

The browser receipt now executes the retained Claude/Codex fake-provider chat spec. The catalog has 34 required selectors; historical loop IDs VL-06, VL-08, and VL-09 are retired instead of being relabeled as covered.

This is a reduction in coverage, not proof that the old combined workflows work. In particular, automated conflict-merge guest execution is a declared coverage gap. Some merge UI/API/template code still exists, including prompts that refer to deleted commands. A green CI result must not be presented as validation of that path. Automatic canonical citation derivation from native provider tool events is also a coverage gap, not a passing worker capability. Ordinary job publication is repaired and tested on its actual assigned refs; it is not redirected to a workspace ref to bypass authorization.

Optional live-LLM tests are outside these deterministic CI checks. No product API, UI feature, or deployment setting is removed by this test retirement.

## Coverage retired with the removed features

G5-Lean has no ontology, grounding graph or persona views, and the current work
is backend-first. CI stopped scheduling the tests for those features because
they were the bulk of the E2E and browser runtime. The unscheduled ones were
deleted on 2026-09-25 and remain in git history at `fd9fba3a`:

- the Rust E2E scenarios `ontology_format_contract` and
  `ontology_validation_phase_1`, which were never in the `retained_scenarios`
  lane of `scripts/run-rust-e2e-lanes.sh`; `ontology_lifecycle` and the
  grounding modules stay in the lane;
- the retired Playwright specs `ontology`, `ontology-format`,
  `grounding-traceability`, `multi-repo-grounding-traceability`,
  `trace-navigation`, `compliance-trace-navigation`,
  `compliance-multi-repo-navigation`, `tinydb-sync-wiring`,
  `chat-intent-workflow`, `chat-task-center-panel` and `add-repo`, together
  with the `G5_E2E_INCLUDE_RETIRED` switch and the `multirepo-grounding`
  browser fixture seed that only `multi-repo-grounding-traceability` read.

Cloud Build steps deleted outright, because each one only re-ran coverage that
is either retired or already covered elsewhere:

- `ontology-v4-validation` — its unit tests also run in the orchestrator's
  `rust-test` lane; only the corpus validation was unique to it, and
  `scripts/ci/run-ontology-corpus-validation.sh` is still in the tree.
- `ontology-v4-api-integration` — `api_ontology_format` is a `g5-api` test
  binary, so the orchestrator's integration lane still runs it.
- `verify-grounding-integrity` — `scripts/verify_grounding.py` is still in the
  tree and runnable.
- `ontology-v4-playwright-e2e` and its stage receipt; the `multirepo-grounding`
  browser fixture seed went with it. `provider-agent-playwright-e2e` now waits
  on the two broad shards directly, since that lane used to sit between them.

Retained on purpose: `sync_job_one_task`, `live_host_sync_merge`,
`mapping_driven_fanout`, `aql_job_submission` and `golden_path` exercise job
admission, task-graph and live-worker semantics that Lean keeps, even though
their names predate the ontology removal.
