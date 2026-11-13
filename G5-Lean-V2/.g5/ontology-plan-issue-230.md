# Issue 230: conversation context recovery

## Ontology changes and governance

**Ontology changes:** Compared with `origin/main` at `2bd71661`, no ontology blocks change. The current contracts already require complete native generations, deterministic bounded delivered-history reconstruction, frozen evidence, provider-neutral conversation carry, and continuation without replaying the original input.

**Relevant and governing ontology:**

| Commitment | Obligation, owner and outcome |
|---|---|
| [g5.conversation.bundle.publication](ontology/features/conversations.g5.md#publish-only-complete-session-generations) | The session store may offer only a complete current generation; partial/failed harvest cannot expose an older bundle as current. |
| [g5.conversation.recovery](ontology/features/conversations.g5.md#recover-a-conversation-from-known-delivered-history) and [g5.conversation.recovery.exclude](ontology/features/conversations.g5.md#exclude-undelivered-inputs-from-recovery) | Agent dispatch supplies a bounded, deterministic context from positively delivered history for both providers when native state is unavailable. Queued, uncertain and withdrawn-only inputs are excluded. |
| [g5.conversation.recovery.snapshot](ontology/features/conversations.g5.md#freeze-recovery-evidence-with-the-reserved-prompt) and [g5.conversation.inbox.prompt](ontology/features/conversations.g5.md#freeze-the-bounded-provider-prompt) | The reserved attempt keeps exact prompt/context bytes and evidence cutoff under the 512 KiB cap; retry cannot absorb later facts. |
| [g5.conversation.recovery.carry](ontology/features/conversations.g5.md#carry-selected-conversation-history-into-a-fresh-session) | ChatService copies selected delivered history without provider identity or queued inbox; a fresh provider turn receives that context. |
| [g5.conversation.continue](ontology/features/conversations.g5.md#continuation-says-what-is-being-continued) and [g5.agents.lifecycle.continuation](ontology/features/agents.g5.md#continue-the-same-logical-work-with-a-fresh-attempt) | TaskService continues the same task in a new attempt using a continuation input. Previously delivered input is context, not a new action. |
| [g5.architecture.conversations](ontology/software_architecture/agents.g5.md#shared-conversations-and-inbox), [g5.architecture.session_store](ontology/software_architecture/agents.g5.md#opaque-provider-session-storage) and [g5.architecture.provider_adapter](ontology/software_architecture/agents.g5.md#provider-protocol-adapter) | ChatService owns durable history, TaskService owns reservation/delivery, session storage owns opaque files, and the adapter selects actual provider resume behavior. |
| [g5.data.identity.generations](ontology/concepts/data_model.g5.md#keep-harvest-and-delivery-generations-distinct), [g5.quality.review](ontology/concepts/coding_principles.g5.md#review-the-changed-contract-and-its-consumers) and [g5.quality.evidence.claims](ontology/concepts/coding_principles.g5.md#distinguish-inspection-from-demonstrated-behavior) | Separate harvest completeness from input delivery, review alternate callers, and report test limits accurately. |

## Outcome and design

Every new turn chooses one explicit context path: a verified current native bundle, deterministic reconstruction from durable positively delivered transcript history, or a visible context-unavailable error. The current per-file upload protocol has no atomic complete-generation publication, so this change chooses reconstruction for every follow-up and does not admit any native bundle as verified. Residual local files, a partial upload and a generation number cannot override that choice. Native resume can return only after a separate complete-generation protocol is implemented and verified.

At reservation, freeze a bounded reconstruction snapshot and the current input separately. Render historical user and assistant text as quoted prior context with clear roles and message indices, not as commands to execute again. Apply the existing 512 KiB prompt limit deterministically, retain the newest eligible messages plus the original continuation objective, report omitted older content, and include bounded attachment names, hashes and scan states while clearly saying bytes are unavailable unless independently attached to the current turn. If durable evidence is insufficient to recover an earlier objective, fail visibly rather than imply full continuity.

Chat forks and workspace conversation carry copy only eligible delivered history to fresh provider sessions. Interrupted-task Continue uses its immutable continuation input and prior delivered history to recall the objective, but it must not redeliver the original user input as a new action; side-effect safety remains governed by task/attempt and operation replay identity.

The UI and timeline distinguish verified native resume, reconstruction with any truncation or attachment limits, and context-unavailable failure. Restore and harvest failures may lower continuity but cannot silently switch to an unrelated conversation.

## Observable acceptance

- A deterministic provider recalls an earlier fact when native files are missing, unusable, or not durably persisted, and after a chat fork.
- Continue resumes the same objective and does not issue the completed input as a second user action.
- Partial files, failed harvest and stale local files cannot become a purported complete native session.
- Tests cover truncation and attachment reference rendering, positive delivery filtering, and truthful notices.
- A continuation with an original objective longer than the ordinary per-message truncation cap retains the full objective or fails before provider handoff; an auth-failure notice without accepted input never becomes provider history.
- Local boundary tests and relevant crate/guard checks pass before independent CLI review; required cloud Rust and E2E CI validate the PR head.

## Risks and source checks for implementation

- Inspect the actual session-file load/upsert/harvest protocol, provider adapter restore behavior, reservation lifecycle, transcript schema, fork copy and UI notices before reusing them. Ontology authority does not establish code compatibility.
- The present harvest protocol uploads files individually. Keep its stored bytes private but never treat its generation value as complete-publication proof. This is the reason native resume stays disabled in this change.
- A transcript can contain incomplete assistant output or missing attachment bytes. Label limits and avoid converting those into executable instructions.
- Check existing migrations and guards before adding persistence; use a forward migration if durable schema must change.

## Design review

Allowed: the current turn receives frozen delivered context; a future complete-generation implementation may restore native resume. Forbidden: partial or stale files resume, queued/uncertain input enters history, or Continue repeats a completed action. Failure/retry: the same reservation uses the same frozen reconstruction bytes and reports that choice. Exception: genuinely insufficient durable evidence fails visibly. The owners and rules above cover each outcome; exact schema remains an implementation detail.
