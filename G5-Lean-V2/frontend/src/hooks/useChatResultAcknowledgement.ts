// =============================================================================
// Chat result acknowledgement
// =============================================================================
// Decides when the person has actually seen a chat's latest finished result
// (chat g5.chat.inspection.acknowledge).
// =============================================================================

import { useEffect, useRef, useState } from "react";

import { transcriptShowsResult } from "../lib/chatSessionActivity";
import type {
  ChatMessageDTO,
  ChatSessionActivityDTO,
  ChatSessionResultDTO,
} from "../lib/types";

function resultKey(sessionId: string, result: ChatSessionResultDTO): string {
  return `${sessionId}:${result.taskId}:${result.attempt ?? "-"}`;
}

/** The page is visible and focused, so what the chat shows can be seen. */
function useDocumentAttention(): boolean {
  const read = () =>
    typeof document !== "undefined" &&
    document.visibilityState === "visible" &&
    document.hasFocus();
  const [attentive, setAttentive] = useState(read);
  useEffect(() => {
    const update = () => setAttentive(read());
    document.addEventListener("visibilitychange", update);
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    return () => {
      document.removeEventListener("visibilitychange", update);
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
    };
  }, []);
  return attentive;
}

/**
 * Acknowledge the chat's latest finished result once it is displayed
 * (chat g5.chat.inspection.acknowledge): the page is visible and focused,
 * the chat surface is shown and scrolled to its end, and the transcript holds
 * that result's finished output. Selecting or mounting the chat proves
 * nothing by itself, so a result finishing later in a background chat or an
 * unfocused page stays uninspected until those conditions hold. Each result
 * is acknowledged at most once per mount; the server ignores older ones. A
 * failed request does not count, so the next re-evaluation sends it again.
 */
export function useChatResultAcknowledgement({
  sessionId,
  activity,
  messages,
  messagesReady,
  surfaceVisible,
  atBottom,
  onAcknowledge,
}: {
  sessionId: string | null;
  activity: ChatSessionActivityDTO | null | undefined;
  messages: readonly ChatMessageDTO[];
  /** The transcript has loaded for this chat. */
  messagesReady: boolean;
  surfaceVisible: boolean;
  atBottom: boolean;
  onAcknowledge?: (
    sessionId: string,
    result: ChatSessionResultDTO,
  ) => Promise<unknown> | void;
}): void {
  const attentive = useDocumentAttention();
  const acknowledgedRef = useRef(new Set<string>());
  const result = activity?.unseenResult ? activity.latestResult : null;
  const displayed =
    !!sessionId &&
    !!result &&
    !!onAcknowledge &&
    attentive &&
    surfaceVisible &&
    atBottom &&
    messagesReady &&
    transcriptShowsResult(messages, result);
  useEffect(() => {
    if (!displayed || !sessionId || !result || !onAcknowledge) return;
    const key = resultKey(sessionId, result);
    if (acknowledgedRef.current.has(key)) return;
    acknowledgedRef.current.add(key);
    void Promise.resolve(onAcknowledge(sessionId, result)).catch(() => {
      acknowledgedRef.current.delete(key);
    });
  }, [displayed, onAcknowledge, result, sessionId]);
}
