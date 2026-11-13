# Issue 232 implementation review

## Scope and mode

Self-review plus one independent read-only Claude CLI review of the uncommitted `codex/issue-232` candidate against `origin/main`. The CLI review was requested only after local verification. The review skill's three parallel discovery contexts were not used; the requested Claude CLI review supplies one fresh context. No review repair has been applied at this point.

The [saved design](ontology-plan-issue-232.md#ontology-changes-and-governance) records the original ontology diff and governing commitments. The [request](chat_intent/issue-232/request.md) and issue 232 acceptance are the review basis. Claude's raw review is retained at `/Users/rahul/.claude/plans/review-the-uncommitted-implementation-synchronous-salamander.md`.

## Initial findings and disposition before repair

| Finding | Governing obligation and actual path | Counterexample or check | Initial disposition |
| --- | --- | --- | --- |
| F1, medium: early `chat.message_updated` can replace newer streamed text with stale canonical content | `g5.conversation.transcript.presentation`; `agent_tasks.rs::publish_agent_event` published an active chat refetch on every `message.final`; `ws.ts::invalidateChatSessionCaches` and `useChat.ts` tail-window merge let the fetched row win. | After a first block final, a pending refetch can resolve while a later block's `chat.chunk` deltas are being appended; the fetched row contains only the earlier block. | Accepted. The intermediate notification was removed; the task timeline still publishes that block and outcome still publishes `ChatMessageUpdated`. A focused ChatPanel test now checks an earlier final plus a later streaming delta and replacement by its completed block. |
| M1, low: terminal result dedup uses equality against a unit or common joined forms | `g5.conversation.transcript.terminal`; `runtime.rs::project_event`. | A hypothetical result containing all completed blocks plus additional answer text could duplicate the earlier blocks. The inspected Claude and Codex result contracts return completed text or the last message, and the required result-only and repeated cases pass. | No change for unsupported hypothetical shape; preserve as a validation limit. |
| M2, low: old unlabelled final events have only adjacency pairing | `g5.conversation.transcript.presentation`; Rust and browser legacy fallbacks. | Duplicate legacy finals with no native identity could appear twice if newly reduced; preexisting canonical rows are already materialized and loaded directly. | No migration or history rewrite in issue scope. |
| M3: the reducer is implemented for Rust journal storage and TypeScript timeline presentation, with `chat.chunk` as a separate live append path | `g5.conversation.transcript.presentation`; API reducer, provider output projection, WebSocket cache. | F1 is a concrete divergence in the live path. | Repair F1, then recheck the three consumers. |

## Verification available before review

The focused dispatcher and API regressions passed; API lifecycle 42/42; affected Rust libraries 1014/1014, 36/36 and 132/132; architecture and inventory 49/49; API route guards 6/6; related frontend tests 336/336; focused Chromium provider cases 2/2; frontend build, Rust workspace check, strict all-target Clippy, Rust format and diff checks passed. Frontend lint had zero errors and 554 warnings. Changed frontend files pass Prettier; the repository-wide check flags 19 unchanged files.

The broader dispatcher suite's sole failing fixture could not load `libkrun.1.dylib` in its child; its exact selector passed with no default features. The automatic Playwright Vite start timed out; both focused browser cases passed with an explicitly started local server. Live provider and VM execution remain unverified locally. Cloud Rust and E2E gates remain pending on the final PR head.

## Commitment ledger after repair

| Commitment | Actual path | Counterexample or check | Result or remaining gap |
| --- | --- | --- | --- |
| `g5.conversation.transcript.units` | Claude/Codex frame mappers → g5d-agent-turn journal → TaskService reducer. | Multiple native IDs, tool separation, duplicate frame and delta/final replacement in mapper, dispatcher and API tests. | Passed synthetic dispatcher/API; live provider/VM unverified. |
| `g5.conversation.transcript.terminal` | Runtime emits a stored synthetic result unit only for new answer text; final reference hashes emitted unit content. | Result-only and repeated final text in dispatcher and API tests. | Passed; unobserved provider result variants unverified. |
| `g5.conversation.transcript.presentation` | TaskService canonical reducer, task timeline projection, providerOutput reduction and ChatPanel rendering; notification after settlement. | Canonical two-block row, timeline, replay, interrupted row, early final plus later partial block, reload Chromium for both providers. F1 traces the old active-refetch path to `useChat.ts` and confirms the early notification is removed at its producer. | Passed tested paths; live multi-block WebSocket timing remains unverified. |
| `g5.agents.outcome.message-status`, `.placeholders` | TaskService outcome/VM-loss settlement and agent event publication. | API result-only, failure, interruption and VM-loss rows; success notification derives from authoritative task status. | Passed covered API cases; suspension and old-attempt matrix unverified. |
| `g5.agents.invoke.journal`, `g5.data.platform-authority`, `g5.design.commit-notify` | TaskService journal and projection in one transaction; `publish_agent_event` reads the committed row. | API seq replay and canonical row count; source trace of post-commit publication. | Passed tested replay; no live VM replay. |
| `g5.story.read-previous-conversations-without-a-vm`, `g5.conversation.recovery` | Persisted `chat_messages` and task timeline feed REST and ChatPanel reload. | API canonical content/status; focused Chromium reload for Claude/Codex. | Passed tested reading path; later prompt recovery from a real VM not run. |
| `g5.architecture.provider_adapter`, `.task_owner` | Provider mappers own native protocol identity; TaskService owns storage and status. | Source review plus crate architecture/inventory guard. | Passed guard; live provider contracts unverified. |

F1 has one local repair pass. No second Claude pass was needed after the source-level producer fix and focused rechecks; no significant finding remains in the reviewed diff. The repair did not change the governing ontology meaning. New and affected metadata entries now record implementation locations and remaining runtime limits. Final workspace checks and Cloud CI are recorded separately after execution.

Post-repair checks: API core library 1014/1014, API lifecycle 42/42, related frontend Vitest 342/342, frontend production build, `cargo check --workspace`, strict workspace all-target Clippy, Rust format, changed-file Prettier and `git diff --check` passed. Changed-file ESLint had zero errors and 15 existing warnings. Cloud CI had not run at this review point.

## PR review and first Cloud run

The first PR head `d6f293ba` passed Cloud `g5-ci-rust` (`ab99a75b-d148-4e54-87d1-1191ef19556f`, 14m24s) and `g5-ci-e2e` (`b3bb188a-223d-4ab5-9324-e0d0c3a2b3c5`, 31m50s). CodeRabbit left one in-scope performance suggestion: preserve every event kind as a legacy pairing boundary, but project JSON `null` instead of transferring full payloads for non-message events in `collect_assistant_message_content`. The suggestion was accepted. The focused canonical multi-block API test and full 42-test lifecycle suite passed with the query refinement. The revised PR head still requires its own Cloud gates.
