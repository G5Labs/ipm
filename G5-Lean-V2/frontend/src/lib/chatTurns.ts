// =============================================================================
// G5 Client — chat turn helpers shared by every workspace Chat surface
// =============================================================================
// The workbench (`WorkspacePage`) and the Home personal chat side panel
// (personal_workspace g5.personal.home) mount the same `ChatPanel` through the
// same rules: which providers are offered, which live timeline entries belong
// to the open session, and which assistant turn a Stop targets.
// =============================================================================

import type {
  ChatAgentProvider,
  ChatMessageDTO,
  ChatSessionDTO,
} from "./types";

// Both shipped providers are available in G5 Lean.
export function supportedCloudAgentProviders(): ChatAgentProvider[] {
  return ["claude_code", "codex"];
}

/**
 * The agent a new chat starts on: the configured default when available;
 * otherwise Auto prefers the first available agent that can authenticate
 * (an assigned account or raw keys, chat g5.chat.account-defaults), then
 * the first available agent.
 */
export function getPreferredAvailableAgentProvider(
  availableAgentProviders: ChatAgentProvider[],
  defaultAgentProvider?: ChatAgentProvider | null,
  authenticatableAgentProviders?: ReadonlyArray<ChatAgentProvider> | null,
): ChatAgentProvider {
  if (
    defaultAgentProvider &&
    availableAgentProviders.includes(defaultAgentProvider)
  ) {
    return defaultAgentProvider;
  }
  const authenticatable = availableAgentProviders.find((provider) =>
    authenticatableAgentProviders?.includes(provider),
  );
  if (authenticatable) {
    return authenticatable;
  }
  if (availableAgentProviders.length > 0) {
    return availableAgentProviders[0];
  }
  return "claude_code";
}

export function shouldAttachTaskTimelineToChatSession(
  taskId: string | null | undefined,
  session:
    | (Pick<ChatSessionDTO, "scopeType" | "scopeRef"> & {
        id?: string;
      })
    | null
    | undefined,
  entrySessionId?: string | null,
): boolean {
  if (!session) return false;
  if (entrySessionId) {
    return !!session.id && entrySessionId === session.id;
  }
  void taskId;
  return false;
}

export function assistantMessageToStop(
  messages: readonly Pick<ChatMessageDTO, "id" | "role" | "status">[],
) {
  return messages.find(
    (message) =>
      message.role === "assistant" &&
      (message.status === "pending" || message.status === "streaming"),
  );
}

/** Platform notices and follow-up inputs do not change the latest turn's state. */
export function hasLiveAssistantTurn(
  messages: readonly Pick<ChatMessageDTO, "role" | "status">[],
): boolean {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role === "assistant") {
      return message.status === "pending" || message.status === "streaming";
    }
  }
  return false;
}

/**
 * True when an assistant turn *after* the stopped turn currently has `status`.
 * "Successor" is strictly positional: only turns later than the stopped message
 * in the ordered timeline count, and a `stoppedMessageId` that is not present
 * yields no successors. Shared by `stopHandoffBridging` and the stop-handoff
 * clearing effect so both use one definition of a queued/streaming successor
 * (chat_panel_runtime D10).
 */
export function chatSuccessorExists(
  messages: readonly Pick<ChatMessageDTO, "id" | "role" | "status">[],
  stoppedMessageId: string,
  status: "pending" | "streaming",
): boolean {
  const stoppedIndex = messages.findIndex(
    (message) => message.id === stoppedMessageId,
  );
  if (stoppedIndex < 0) return false;
  for (let index = stoppedIndex + 1; index < messages.length; index += 1) {
    const message = messages[index];
    if (message.role === "assistant" && message.status === status) return true;
  }
  return false;
}

/**
 * Stop-control suppression is scoped to an accepted A→B turn handoff only
 * (chat_panel_runtime D10). After Stop targets assistant turn A, the composer
 * Stop control is hidden only while a queued successor turn B (a later,
 * still-pending assistant turn) is coming, and it reappears the moment B starts
 * streaming. A plain stop with no queued successor is not a handoff, so it must
 * never be suppressed — otherwise the single Stop control vanishes while the
 * turn is still live and only a page refresh (which drops the in-memory
 * stopHandoff) brings it back. Bridging is judged only from turns after the
 * stopped turn, so a pending predecessor never counts as a handoff.
 *
 * Returns true only while a queued (pending) successor turn exists behind the
 * stopped turn and no successor is streaming yet.
 */
export function stopHandoffBridging(
  messages: readonly Pick<ChatMessageDTO, "id" | "role" | "status">[],
  stoppedMessageId: string,
): boolean {
  return (
    chatSuccessorExists(messages, stoppedMessageId, "pending") &&
    !chatSuccessorExists(messages, stoppedMessageId, "streaming")
  );
}
