// =============================================================================
// G5 Client — one Stop state per chat turn (chat g5.chat.controls)
// =============================================================================
// Shared by the workbench and the Home chat: `Stopping…` appears as soon as a
// Stop is sent, no second request leaves for the same turn while one is in
// flight, the runtime's disposition decides what the person sees next, and a
// request that did not commit offers Retry. The accepted A→B handoff fence
// (chat_panel_runtime D10) is kept unchanged.
// =============================================================================

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  assistantMessageToStop,
  chatSuccessorExists,
  stopHandoffBridging,
} from "../lib/chatTurns";
import type { ChatMessageDTO } from "../lib/types";
import { useStopStreaming } from "./useChat";

export type ChatStopPhase =
  /** The request is in flight; no second request for this turn. */
  | "requesting"
  /** The stop intent is committed and the runtime is signalling the turn. */
  | "stopping"
  /** The execution had already finished; its output is being saved. */
  | "finishing"
  /** The intent committed but the runtime call failed; delivery is retried. */
  | "undelivered"
  /** The request failed before the intent committed; Retry is offered. */
  | "failed";

export interface ChatStopState {
  sessionId: string;
  messageId: string;
  phase: ChatStopPhase;
  error: string | null;
}

type Turn = { sessionId: string; messageId: string };

export interface ChatStopController {
  stopState: ChatStopState | null;
  /** Hide Stop during an accepted turn handoff (chat_panel_runtime D10). */
  stopControlSuppressed: boolean;
  onStop: () => void;
}

export function useChatStopController({
  sessionId,
  messages,
  messagesReady = true,
}: {
  sessionId: string | null | undefined;
  messages: readonly ChatMessageDTO[];
  /** False while the transcript is still loading, so a handoff is not judged
   * against an empty list. */
  messagesReady?: boolean;
}): ChatStopController {
  const stopStreaming = useStopStreaming();
  const stopStreamingAsync = stopStreaming.mutateAsync;
  const stopHandoffRef = useRef<Turn | null>(null);
  const [stopHandoff, setStopHandoff] = useState<Turn | null>(null);
  const stopStateRef = useRef<ChatStopState | null>(null);
  const [storedStopState, setStoredStopState] = useState<ChatStopState | null>(
    null,
  );
  const setStopState = useCallback((next: ChatStopState | null) => {
    stopStateRef.current = next;
    setStoredStopState(next);
  }, []);

  // The ref and the state are set and cleared together, so the state alone
  // judges suppression (reading the ref during render is not allowed).
  const stopControlSuppressed = Boolean(
    stopHandoff &&
    stopHandoff.sessionId === sessionId &&
    stopHandoffBridging(messages, stopHandoff.messageId),
  );

  useEffect(() => {
    const stoppedTurn = stopHandoffRef.current;
    if (!stoppedTurn) return;
    const clearHandoff = () => {
      stopHandoffRef.current = null;
      setStopHandoff((current) => (current === stoppedTurn ? null : current));
    };
    if (stoppedTurn.sessionId !== sessionId) {
      clearHandoff();
      return;
    }
    if (!messagesReady) return;
    const successorStreaming = chatSuccessorExists(
      messages,
      stoppedTurn.messageId,
      "streaming",
    );
    const successorQueued = chatSuccessorExists(
      messages,
      stoppedTurn.messageId,
      "pending",
    );
    const stoppedTurnStillLive = messages.some(
      (message) =>
        message.id === stoppedTurn.messageId &&
        (message.status === "pending" || message.status === "streaming"),
    );
    // The handoff bridge is over once the successor took over (B streaming)
    // or no bridge remains (the stopped turn ended and nothing is queued), so
    // a stale entry never suppresses a later turn.
    if (successorStreaming || (!stoppedTurnStillLive && !successorQueued)) {
      clearHandoff();
    }
  }, [messages, messagesReady, sessionId]);

  // The state belongs to one turn: it ends with the turn (the message left
  // pending/streaming, or the session changed), never on a timer. Derived at
  // render so no effect writes state.
  const stopState = useMemo(() => {
    const current = storedStopState;
    if (!current || current.sessionId !== sessionId) return null;
    if (current.phase === "requesting" || !messagesReady) return current;
    const target = messages.find((message) => message.id === current.messageId);
    const live =
      !!target &&
      (target.status === "pending" || target.status === "streaming");
    return live ? current : null;
  }, [messages, messagesReady, sessionId, storedStopState]);

  const onStop = useCallback(() => {
    if (!sessionId) return;
    // Fence re-entry only while an accepted A→B handoff is still bridging.
    if (
      stopHandoffRef.current &&
      stopHandoffBridging(messages, stopHandoffRef.current.messageId)
    ) {
      return;
    }
    const active = assistantMessageToStop(messages);
    if (!active) return;
    const inFlight = stopStateRef.current;
    if (
      inFlight &&
      inFlight.sessionId === sessionId &&
      inFlight.messageId === active.id &&
      inFlight.phase === "requesting"
    ) {
      return;
    }
    const turn: Turn = { sessionId, messageId: active.id };
    stopHandoffRef.current = turn;
    setStopHandoff(turn);
    const requested: ChatStopState = {
      ...turn,
      phase: "requesting",
      error: null,
    };
    setStopState(requested);
    void stopStreamingAsync(turn)
      .then((result) => {
        if (stopStateRef.current !== requested) return;
        const disposition = result?.disposition;
        if (disposition === "already_ended") {
          setStopState(null);
          return;
        }
        const phase: ChatStopPhase =
          disposition === "already_finished"
            ? "finishing"
            : disposition === "undelivered"
              ? "undelivered"
              : "stopping";
        setStopState({ ...turn, phase, error: null });
      })
      .catch((error: unknown) => {
        if (stopHandoffRef.current === turn) {
          stopHandoffRef.current = null;
          setStopHandoff((current) => (current === turn ? null : current));
        }
        if (stopStateRef.current !== requested) return;
        setStopState({
          ...turn,
          phase: "failed",
          error: error instanceof Error ? error.message : String(error),
        });
      });
  }, [messages, sessionId, setStopState, stopStreamingAsync]);

  return { stopState, stopControlSuppressed, onStop };
}

/** The label the streaming indicator shows for a turn being stopped. */
export function chatStopActivityLabel(
  state: Pick<ChatStopState, "phase"> | null | undefined,
): string | null {
  switch (state?.phase) {
    case "requesting":
    case "stopping":
      return "Stopping…";
    case "finishing":
      return "Finishing… saving the reply";
    case "undelivered":
      return "Stop recorded, delivering…";
    default:
      return null;
  }
}
