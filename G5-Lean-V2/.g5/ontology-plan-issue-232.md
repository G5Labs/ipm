# Issue 232: durable assistant transcript

## Ontology changes and governance

**Comparison base:** `origin/main` at `e5268913`.

**Ontology changes:** Add `g5.conversation.transcript.units`, `.terminal` and `.presentation` to [conversations](ontology/features/conversations.g5.md). No existing commitment changes meaning. The actual ontology diff at the coding handoff is:

```diff
@@ -144,6 +144,24 @@
 Positive input acceptance creates its delivered transcript row and assistant placeholder atomically.

+### Preserve completed assistant units
+
+[D · g5.conversation.transcript.units]
+
+Project each completed native assistant message, item and text block in provider order with its own stable identity. A completed block is durable output, not proof that the turn succeeded; partial output remains visible when execution stops or fails. Replayed events cannot duplicate a unit.
+
+### Resolve terminal answer text
+
+[D · g5.conversation.transcript.terminal]
+
+When the terminal provider result supplies answer text, persist a resolvable assistant unit for text not already represented by completed units. Repeated final text must not duplicate the answer. Derive the final-message reference from stored content, not from an unmaterialized result string.
+
+### Keep streaming and history in agreement
+
+[D · g5.conversation.transcript.presentation]
+
+Use one deterministic transcript reduction for live presentation and durable chat history so reconnect and replay preserve the same ordered assistant text and unit identities. Settled task outcome governs assistant status under [message settlement](agents.g5.md#project-assistant-status-from-settled-outcome), even when text was emitted earlier.
+
 ### Delivery means actual transport handoff
@@ -431,6 +449,24 @@
 - From [C117–C118](../../uplift/g5-lean/candidates/contracts.md).
 - ~85% · input acceptance ingestion; old-attempt acceptance is rejected instead of retained as fact.

+### g5.conversation.transcript.units
+
+- human-verified
+- [Issue 232](../../chat_intent/issue-232/request.md) requires native unit identity, ordering and partial output without conflating block and turn completion.
+- Unassessed · implementation and end-to-end presentation have not yet been reviewed for this change.
+
+### g5.conversation.transcript.terminal
+
+- human-verified
+- [Issue 232](../../chat_intent/issue-232/request.md) requires resolvable terminal answers and deduplication.
+- Unassessed · terminal-result persistence has not yet been reviewed for this change.
+
+### g5.conversation.transcript.presentation
+
+- human-verified
+- [Issue 232](../../chat_intent/issue-232/request.md) requires the same ordered text across stream, reload and replay.
+- Unassessed · shared reduction and browser rendering have not yet been reviewed for this change.
+
### g5.conversation.delivery
```

**Review reconciliation:** The three new footer assessments now identify the implemented provider, journal, API and browser paths and the live provider/VM limits. Existing `g5.story.read-previous-conversations-without-a-vm` and `g5.agents.outcome.message-status` footer assessments were updated because this implementation covers failure, interruption and VM-loss rows in the tested API path. Their governing commitments did not change. Claude's review found that an intermediate canonical chat refetch could overwrite newer streaming deltas; the final design publishes completed blocks through the task timeline and refetches canonical chat at settled outcome. The [review record](ontology-review-issue-232.md) retains the finding, repair and checks.

**Relevant and governing ontology:**

| Commitment | Obligation and affected operation | Allowed case and counterexample | Conclusion |
| --- | --- | --- | --- |
| [g5.conversation.transcript.units](ontology/features/conversations.g5.md#preserve-completed-assistant-units) | Agent adapter and TaskService preserve each completed native unit in order. | Two blocks retain both identities; one task/attempt ID reused for both violates it. | Added. |
| [g5.conversation.transcript.terminal](ontology/features/conversations.g5.md#resolve-terminal-answer-text) | Terminal answer gets durable text; exact repetition is idempotent. | Result-only answer resolves; a hash with no stored text violates it. | Added. |
| [g5.conversation.transcript.presentation](ontology/features/conversations.g5.md#keep-streaming-and-history-in-agreement) | Durable and live consumers reduce the same ordered units. | Reconnect shows the same text; stale or duplicate blocks violate it. | Added. |
| [g5.agents.outcome](ontology/features/agents.g5.md#provider-output-is-not-the-final-execution-decision), [message-status](ontology/features/agents.g5.md#project-assistant-status-from-settled-outcome), [placeholders](ontology/features/agents.g5.md#settle-every-accepted-input-placeholder) | TaskService settles final status from outcome, including partial output. | Interrupted text is stopped; a completed block marked success early violates it. | Preserve. |
| [g5.agents.invoke.journal](ontology/features/agents.g5.md#commit-event-projection-with-acknowledgement) | Journal ingestion, projection and acknowledgement share a commit. | Replay cannot duplicate; a side write outside ingestion violates it. | Preserve. |
| [g5.story.read-previous-conversations-without-a-vm](ontology/features/conversations.g5.md#read-previous-conversations-without-a-vm), [g5.conversation.recovery](ontology/features/conversations.g5.md#recover-a-conversation-from-known-delivered-history) | Durable history and future provider context use the settled transcript. | Reload retains all text; prompt recovery from an incomplete canonical row violates it. | Preserve. |
| [g5.data.platform-authority](ontology/concepts/data_model.g5.md#the-api-owns-durable-platform-records), [outcome-evidence](ontology/concepts/data_model.g5.md#report-outcomes-from-their-authoritative-owners), [g5.design.commit-notify](ontology/concepts/system_design_principles.g5.md#notify-after-commit) | API owns canonical rows and publishes committed facts; provider output does not claim task success. | Stream reflects committed output and settled status; a provisional notification treated as authority violates it. | Preserve. |
| [g5.architecture.provider_adapter](ontology/software_architecture/agents.g5.md#provider-protocol-adapter), [g5.architecture.task_owner](ontology/software_architecture/agents.g5.md#one-logical-task-owner) | Adapter translates native units; TaskService owns projection and settlement. | Both Claude and Codex use shared contract; API reconstructing provider-specific frames violates it. | Preserve. |

## Requested outcome and scope

Implement issue 232 for the native agent turn, journal/API projection, canonical chat materialization, and browser consumers. Keep task timeline and chat text consistent across normal completion, tool/commentary turns, result-only answers, repetition, interruption and replay. Related recovery and interactive issues are context, not blanket scope expansion.

## High-level design

Translate native provider events to identity-bearing completed units. Reduce those units deterministically in their observed order and use that reduction for durable transcript materialization and live rendering. Reconcile terminal result text against completed units, adding a durable unit only for missing answer text. Let authoritative task settlement set the chat status after output closure; partial output remains visible with stopped/error status. The exact policy and effects of existing journal, chat and stream operations require source inspection during coding.

Publish each completed block through the committed task timeline. Refresh the canonical chat message at terminal settlement so an active REST refetch cannot replace newer live deltas from a later block.

## Observable acceptance and verification

Exercise real dispatcher/API paths for multiple blocks, commentary then tool and final answer, result-only text, repeated text, and interrupted output. Inspect `chat_messages`, timeline, and browser presentation after reload/reconnect and replay. Run the smallest regression, affected crate/process tests and guards, then one final workspace check, strict Clippy, formatting and diff checks. Request Claude CLI review after these pass, address significant findings in at most six rounds, then run affected checks and one final workspace validation before PR and Cloud CI.

## Risks and open assumptions

Native providers may assign message, item and block identity at different levels; the adapter must preserve meaningful identity without requiring both providers to expose the same frame shape. A terminal result may repeat the whole answer or only the final block; coding must inspect the actual native event contracts and existing consumer semantics before choosing reconciliation mechanics. No unresolved human decision blocks implementation.
