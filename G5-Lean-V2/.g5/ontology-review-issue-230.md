# Issue 230 implementation review

## Scope and review mode

Self-review of the branch against [the pinned design](ontology-plan-issue-230.md), issue 230 and its unchanged governing ontology. Independent review starts only after the implementer's local validation. The implementation crosses TaskService, ChatService, the agent adapter and persisted attempt state.

## Initial self-review findings and disposition

| Finding | Consequence | Repair and evidence |
|---|---|---|
| The first reservation patch updated an immutable attempt policy after insert and reserved an input before its attempt row existed. | PostgreSQL trigger/foreign key rejected dispatch. | Freeze before inserting `task_attempts`, then insert the continuation input and reserve it. Focused API integration passed. |
| The initial recovery query materialized all eligible content and aggregated every attachment for selected messages. | Database work and memory could grow without the intended reconstruction bound. | Select at most 256 latest messages plus a continuation anchor; cap attachment references at 16 per message; count eligible rows separately. Focused API integration passed. |

## Commitment checks

| Commitment | Actual path and discriminating check | Result or gap |
|---|---|---|
| `g5.conversation.recovery`, `.exclude` | `freeze_recovery_prompt` reads terminal durable transcript rows; API regression seeds an incomplete assistant row and confirms exclusion, stale native identity and prior fact in prompt. Runtime transport test checks both provider launch plans. | API and unit checks passed; live-provider semantic recall remains unexecuted. |
| `g5.conversation.recovery.snapshot`, `g5.conversation.inbox.prompt` | Reservation persists exact rendered prompt, hash and cutoff in `task_attempts.policy_snapshot_ref` before dispatch; API regression reads the persisted value. | Passed; later-acceptance retry boundary remains unexecuted. |
| `g5.conversation.recovery.carry`, fork story | `ChatService` copies transcript and attachments with valid status; API test calls the fork route and checks fresh dispatch context. Workspace carry admits both providers by inspection. | Fork API passed; workspace carry public route and live provider unexecuted. |
| `g5.story.continue-interrupted-work` | API test seeds a stopped prior attempt with `persist_failed`, original accepted input and completed side effect; next attempt carries objective in historical context, creates distinct continuation input and does not duplicate original inbox content. | API check passed; live side-effect execution unexecuted. |
| `g5.conversation.bundle.publication`, `g5.architecture.session_store` | The adapter's `resume_id` is always absent because current per-file harvest cannot establish complete publication. API checks assert old identity/generation are absent from dispatch. | Partial or stale bytes cannot be selected for native resume; complete native publication remains open work. |
| `g5.architecture.provider_adapter`, `g5.architecture.conversations` | TaskService freezes context; adapter emits `conversation_reconstructed` or `context_unavailable` before provider launch; ChatService owns transcript copy. | Source and unit/API checks pass; UI rendering of notices remains unexecuted. |

## Local validation before independent review

- Focused recovery tests passed, including stale generation and persist-failed continuation.
- `g5-api-core --lib`: 1007 passed.
- `g5d-agent-turn --lib`: 31 passed.
- `g5-api --test api_task_lifecycle`: 39 passed.
- `g5-api --test migration_p4_1_runtime_schema`: 5 passed.
- `g5d --test dispatch_modules agent_turn_`: two pass; one Codex child process cannot load Homebrew `libkrun` after the test strips `DYLD_LIBRARY_PATH`. This is a macOS local harness dependency; remote Linux CI is required for that path.
- Final architecture/inventory preflight: all 11 phases passed.
- `cargo check --workspace`, strict `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` and `git diff --check`: passed after applying rustfmt.

## Independent findings, round 1

Three fresh read-only ontology reviewers saved their initial reports locally before sharing. The requested Claude CLI reviewer also saved its full report and summary locally. Claude found no hard defect, but found missing process-level coverage of `context_unavailable` and a misleading pre-upgrade error message. It did not identify the long-anchor, auth-notice, selection or attachment counterexamples found by the ontology reviewers. Consolidated initial findings above were saved before repair.

| Finding | Evidence and obligation | Planned disposition |
|---|---|---|
| Original continuation objective is truncated to 16 KiB. | All three reviewers found that `render_recovery_prompt` can launch after dropping decisive tail clauses. This violates `g5.conversation.recovery` and continuation safety. | Preserve the full anchor when it fits the bounded context; otherwise report `context_unavailable`. Add a long-objective regression. |
| Device-auth failure assistant notice enters provider history. | Two reviewers traced `task_lifecycle.rs` materialization before positive acceptance through the recovery query, including assistant-only first history and fork copy. This violates `.recovery.exclude`. | Exclude the undelivered platform notice while retaining visible transcript and accepted stopped output. Add auth-failure regression. |
| Newer correction can be skipped while older fact is retained. | One reviewer found that overflow `continue` backfills older messages, risking a stale answer. This violates deterministic newest-history recovery. | Stop selection at the first omitted newer message; fail visibly if no newest non-anchor message fits. Add a conflicting-fact budget regression. |
| Attachment references beyond 16 per message are omitted without disclosure. | One reviewer found the SQL cap is invisible to the prompt and notice. | Count and disclose omitted attachment references; add a 17-reference regression. |
| Reconstruction notice precedes prompt acceptance. | One reviewer traced the notice before authentication, process spawn and transport handoff. | Emit successful reconstruction notice only after `input.accepted`; verify the failure path does not claim delivery. |

The reviewers also recorded unexecuted deterministic live-provider recall, public workspace-carry and late-acceptance retry. The late-acceptance projection gap predates this branch and remains visible in ontology metadata; it is not treated as proof that recovery works on that path.

## Round 1 dispositions and repair checks

- **Full continuation objective:** the anchor is rendered whole; an anchor beyond the 256 KiB recovery budget returns `context_unavailable`. New unit regression verifies both a 20 KiB tail constraint and visible failure for an oversized objective. Focused API continuation and core tests passed.
- **Auth-failure projection:** assistant `error` rows are excluded from recovery alongside user `error` rows. The source creates those error notices before acceptance, and copied transcripts lose attempt provenance, so conservative status exclusion is the stable cross-fork choice. A new API regression proves an auth-only transcript does not block the next first turn. Accepted `stopped` assistant output remains eligible. The tradeoff is that accepted provider-error partial text is not reconstructed; the visible transcript retains it.
- **Newest context:** rendering stops at the first overflowing newer message and reports unavailable if no newest non-anchor message can fit. A conflicting-fact unit regression prevents an older code from appearing without the correction.
- **Attachment cap:** the frozen prompt and notice now report omitted references after the 16-per-message cap. The API regression inserts 17 references and checks 16 listed, one omitted.
- **Notice timing:** `conversation_reconstructed` now emits after `input.accepted`; early authentication, profile, spawn and transport failures cannot claim a fresh provider session started. The unavailable failure emits before those operations.
- **Claude F1:** a new API regression checks persisted unavailable metadata, and a new `g5d` process regression checks `context_unavailable` outcome/notice with no `input.accepted` or reconstruction notice. The process selector passed locally with the Homebrew loader path. Its first run found a test assertion using `reason` instead of the module outcome's `code`; correcting the assertion and rerunning only that selector passed.
- **Claude F2:** no short-circuit at reservation. The pinned design assigns truthful failure emission to the agent adapter, and terminal failure already prevents a continuation loop; optimizing away one VM turn would add a second settlement path without a required behavior gain.
- **Claude F3:** the pre-upgrade message now says the attempt cannot resume safely, consistent with terminal failure.
- **Claude F4/F5:** the legacy native-ID branch remains as defense for direct or older callers; rolling deployment compatibility is governed by the existing versioned module rollout and is not altered here.

The first review round had no significant remaining disagreement; the CLI's no-hard-defect assessment did not refute the source counterexamples, and the implementer accepted and fixed them.

## Post-repair verification

- Focused core recovery tests: 12 passed.
- Stale native generation with 17 attachments, undelivered auth notice and unavailable continuation API selectors: passed.
- `g5d --test dispatch_modules agent_turn_unavailable_context_fails_before_provider_handoff`: passed with local Homebrew loader path; the first run found only an incorrect test assertion field, fixed and rerun selectively.
- Full affected suites: `g5-api-core --lib` 1009 passed; `g5d-agent-turn --lib` 31 passed; `g5-api --test api_task_lifecycle` 41 passed.
- Final post-repair validation: all 11 preflight phases, `cargo check --workspace`, strict `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` and `git diff --check` passed.
- A refreshed `origin/main` at `2bd71661` adds only wording clarifications in ontology skills. Its ontology commitments and application code match the reviewed base; reconciliation found no semantic conflict.

## Cloud review round 2

CodeRabbit completed on PR 246's first head. Its four comments were evaluated against the actual code and batched locally before another push:

- The committed review record included inaccessible local absolute paths and a developer username. Replaced them with a plain statement that the source reports were saved locally.
- The old `agent.turn` 1.0.0 guest rejects the new `recovery` field because it denies unknown fields. Fresh first-turn input now omits absent recovery; context-bearing turns require module 1.0.1. Updated the guest catalog, manifest identity, pinned hash/version and process test selector. The old guest fails a mismatched version instead of silently accepting a turn without recovery.
- The SQL selection carried full non-anchor bodies despite a 16 KiB render cap. Selection now carries row identity; the outer query fetches at most 16,385 characters per non-anchor row and retains the full continuation anchor. Rust still enforces the byte cap and truncation marker.
- Fork copy reused an attachment helper that rewrote agent screenshots as user uploads. The helper now preserves each source storage prefix in both key and mount path. A public fork API regression checks a copied screenshot keeps `agent-screenshots/`.

Focused fork, first-turn and `g5d` process selectors passed. The changed catalog initially failed its hash-pin test; the catalog hash and API pin were corrected, and only the failed selector was rerun successfully. `g5-api-core --lib` then passed 1009 tests and `g5-api --test api_task_lifecycle` passed 41. All 11 preflight guards passed. Cloud Rust passed on the first PR head; Cloud E2E was still running when this record was updated. Review-and-address round count: two.

## First-head Cloud E2E diagnosis

Cloud Rust passed. Cloud E2E failed three selectors with the same second-turn symptom: Rust `test_two_chats_and_a_queued_turn_share_one_vm_without_releasing_held_work` and `test_streamed_chat_survives_api_restart_and_continues_same_task` did not reach two replies, and Playwright `chat-restart-continuity.spec.ts` saw only one matching assistant message after three attempts. The value-loop receipt failed only because those mandatory selectors failed. The source counterexample is the fake Claude workflow's exact-one `G5_FS_WRITE_SET_E2E` parser: the newly reconstructed prompt contains an earlier write-set and wait directive in historical context plus the new directive in current input. The fake provider treated both as actions, although the recovery prompt explicitly marks history as non-actionable. A focused fake-provider test uses the exact write-set format, checks only the current files are written, and passes; all 41 fake-provider library tests pass. The fixture now scopes deterministic side effects to text after the recovery current-input delimiter. This is a harness assumption exposed by the feature, not evidence that the provider received the wrong conversation. No other Cloud E2E selector failed on the first head.
