# Issue 231: deliver chat attachments to active agent turns

## Ontology changes and governance

**Ontology changes:** Compared with `origin/main` at `7e50b7a0`, add `g5.conversation.inbox.content` in [conversations](ontology/features/conversations.g5.md#deliver-reserved-content-to-the-provider), with its metadata:

```diff
### Deliver reserved content to the provider

[D · g5.conversation.inbox.content]

Carry typed text, image, file and reference content from the reserved input into the frozen invocation. Resolve attachment bytes under current tenant and owner authority, verify their recorded identity and integrity, and give each provider its native supported encoding. Refuse missing, corrupt, unauthorized or unsupported content before claiming delivery; replay of the same reservation must not add or duplicate content.

Recovery may describe prior attachments as history, but does not redeliver their bytes as a new user action. [Reservation](#reserve-one-exact-input-revision), [recovery evidence](#freeze-recovery-evidence-with-the-reserved-prompt) and [delivery](#delivery-means-actual-transport-handoff) govern this content.

### g5.conversation.inbox.content

- human-provided
- [Issue 231](https://github.com/G5Labs/G5-Lean/issues/231) and the [implementation request](../../chat_intent/issue-231/request.md) specify typed content, authorized integrity-checked retrieval, native provider encoding and explicit refusal.
- ~90% · ChatService binds inline bytes to the queued input; agent_dispatch freezes typed descriptors; the internal content route and agent.turn verify authority, identity, hash and image format before native Claude/Codex handoff. Focused synthetic-provider and refusal regressions plus Cloud Rust and E2E gates passed; the public VM image scenario passed with a synthetic provider. Live-provider receipt remains unverified.
```

**Relevant and governing ontology:**

| Commitment | Obligation → owner/operation → allowed case and counterexample |
|---|---|
| [g5.conversation.inbox.reservation](ontology/features/conversations.g5.md#reserve-one-exact-input-revision), [g5.conversation.inbox.content](ontology/features/conversations.g5.md#deliver-reserved-content-to-the-provider) | TaskService freezes one revision and attachment set; the current image/file reaches one invocation; a retry cannot duplicate or silently omit it. |
| [g5.conversation.inbox.prompt](ontology/features/conversations.g5.md#freeze-the-bounded-provider-prompt), [g5.conversation.recovery.snapshot](ontology/features/conversations.g5.md#freeze-recovery-evidence-with-the-reserved-prompt) | Dispatch freezes exact prompt and history; retry cannot incorporate changed content or later facts. |
| [g5.conversation.delivery](ontology/features/conversations.g5.md#delivery-means-actual-transport-handoff), [g5.data.outcome-evidence](ontology/concepts/data_model.g5.md#report-outcomes-from-their-authoritative-owners) | Agent adapter reports acceptance only after native transport; invalid content fails visibly before claimed delivery. |
| [g5.conversation.recovery](ontology/features/conversations.g5.md#recover-a-conversation-from-known-delivered-history), [g5.conversation.recovery.carry](ontology/features/conversations.g5.md#carry-selected-conversation-history-into-a-fresh-session) | Recovery/fork supplies prior attachment descriptions as history while fresh current content is delivered once. |
| [g5.architecture.conversations](ontology/software_architecture/agents.g5.md#shared-conversations-and-inbox), [g5.architecture.provider_adapter](ontology/software_architecture/agents.g5.md#provider-protocol-adapter) | ChatService owns storage, TaskService reservation, dispatch materialization, and adapter-native encoding. |
| [g5.security.tenant](ontology/concepts/security.g5.md#tenant-scope-survives-privileged-operations), [g5.security.admission](ontology/concepts/security.g5.md#admit-actions-against-current-authority) | Artifact lookup must bind organization/workspace/session/current owner; a guessed cross-tenant ID fails. |
| [g5.quality.evidence.fakes](ontology/concepts/coding_principles.g5.md#keep-test-doubles-outside-the-behavior-under-test), [g5.quality.evidence.claims](ontology/concepts/coding_principles.g5.md#distinguish-inspection-from-demonstrated-behavior) | A synthetic provider may check bytes at the active launch path; its pass does not claim live-provider or VM qualification. |

## Outcome and design

The public chat input retains text and its attachment descriptors under one input identity. Dispatch freezes those descriptors into typed image/file/reference invocation entries bound to that input and its recorded hash. The active turn uses the incarnation-scoped internal API to retrieve bytes, checks session/input identity, metadata, length and SHA-256, and materializes files privately. Claude and Codex adapters each encode supported images through their native input mechanisms. Non-image file and reference content receive an explicit prelaunch failure, never text-only success. Shared materialization owns access and integrity checks, not provider encoding.

First-turn request-key identity includes the validated attachment set so a changed image cannot replay the previous accepted input. A malformed stored manifest becomes a reserved turn's explicit failed outcome rather than repeatedly rolling back tenant dispatch. Prelaunch materialization decodes the image under its declared MIME and bounded dimensions before claiming delivery; MIME comparisons are case-insensitive while the native provider value is normalized.

Current-turn attachments travel only with the reserved input. Delivered-history reconstruction, continuation and fork may cite prior attachment metadata as context but do not replay prior bytes as a new action. Retry of the same invocation uses its frozen identity and manifest. Failures before native handoff leave the input undelivered with a clear outcome.

## Observable acceptance

- Public API/browser input carrying an image reaches the active module; a synthetic provider verifies exact prompt and attachment bytes/hash.
- Claude receives image content blocks and Codex receives image input; non-image file behavior is explicit and tested.
- Queued-input linkage, exact retry, continuation, recovery and fork preserve one current attachment delivery.
- Missing, corrupt, unauthorized and unsupported content fail clearly before delivery acceptance.
- Run the requested focused-to-workspace local validation, then independent Claude CLI review, then required Cloud Rust and E2E CI on the PR.

The public API acceptance uses the deterministic provider's `G5_IMAGE_SHA256_E2E` assertion. The real dispatcher regression separately covers both provider launch encodings and exact bytes. Neither test claims live-provider correctness.

## Risks and source checks

Inspect actual storage layout, artifact ownership and hash format, reservation snapshot and invocation serialization before reusing operations. In particular, verify whether the existing retrieval operation authorizes tenant/workspace/session and verifies bytes. Inspect current Claude/Codex launch semantics and size limits rather than assuming older helper support applies. If durable schema changes are necessary, add a forward migration and verify the real migration role. A synthetic provider proves the production launch boundary under its environment; live provider and VM claims require distinct evidence.

## Design review

Allowed: one authorized immutable current image delivered with the reserved prompt. Forbidden: a guessed cross-session artifact, a changed blob with the same ID, an unsupported file quietly dropped, or a replay that adds an attachment. Failure and retry keep the same reservation identity and refuse before accepted delivery. Recovery cites previous content as history without reattaching its bytes. The design uses the current ownership and delivery contracts; implementation must confirm the operations' actual policy and effects.
