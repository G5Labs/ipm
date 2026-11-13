// =============================================================================
// Chat activity: states, times and the displayed-result test
// =============================================================================
// Pure helpers for the chat picker's activity indicator and times (chat
// g5.chat.activity, g5.chat.ui.picker) and for deciding whether a transcript
// shows a chat's latest finished result (g5.chat.inspection.acknowledge). The
// server derives the activity; nothing here infers it from transcript state.
// =============================================================================

import type {
  ChatMessageDTO,
  ChatSessionActivityDTO,
  ChatSessionDTO,
  ChatSessionResultDTO,
} from "./types";

export type ChatActivityState = "active" | "unseen" | "idle";

/** The spinner takes precedence over an uninspected result. */
export function chatActivityState(
  activity: ChatSessionActivityDTO | null | undefined,
): ChatActivityState {
  if (activity?.activeTurnStartedAt) return "active";
  if (activity?.unseenResult) return "unseen";
  return "idle";
}

/**
 * What a chat's bubble above the Chat corner button shows (ui
 * g5.workspace.ui.chat-bubble-stack): `waiting` while the turn waits for the
 * person (g5.chat.activity.awaiting-input), else `working` while a turn is
 * active, else `idle`, at rest or with a result still uninspected. Every
 * chat has a bubble; a missing activity (older servers) reads as idle.
 */
export type ChatBubbleState = "waiting" | "working" | "idle";

export function chatBubbleState(
  activity: ChatSessionActivityDTO | null | undefined,
): ChatBubbleState {
  if (activity?.awaitingInput) return "waiting";
  if (activity?.activeTurnStartedAt) return "working";
  return "idle";
}

export function formatAgo(
  at: string | null | undefined,
  nowMs: number,
): string | null {
  if (!at) return null;
  const atMs = Date.parse(at);
  if (!Number.isFinite(atMs)) return null;
  const diffSeconds = Math.max(0, Math.floor((nowMs - atMs) / 1000));
  if (diffSeconds < 60) return "just now";
  const diffMinutes = Math.floor(diffSeconds / 60);
  if (diffMinutes < 60) return `${diffMinutes}m ago`;
  const diffHours = Math.floor(diffMinutes / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  return `${diffDays}d ago`;
}

/** `Active now`, `Last active 5m ago`, or `No finished turns`; null when the
 * server reported no activity (search results, older servers). */
export function chatActivityTimeLabel(
  activity: ChatSessionActivityDTO | null | undefined,
  nowMs: number,
): string | null {
  if (!activity) return null;
  if (activity.activeTurnStartedAt) return "Active now";
  const lastActive = formatAgo(activity.lastFinishedAt, nowMs);
  return lastActive ? `Last active ${lastActive}` : "No finished turns";
}

/** The picker row's time line: `Started 2h ago · Last active 5m ago`. */
export function chatPickerTimeLine(
  session: ChatSessionDTO,
  nowMs: number,
): string | null {
  const started = formatAgo(session.createdAt, nowMs);
  const activity = chatActivityTimeLabel(session.activity, nowMs);
  return (
    [started ? `Started ${started}` : null, activity]
      .filter(Boolean)
      .join(" · ") || null
  );
}

/** The center view's recent-conversation time: the activity, or when the chat
 * started before any turn finished. */
export function chatRecentTimeLabel(
  session: ChatSessionDTO,
  nowMs: number,
): string | null {
  const activity = session.activity;
  if (activity?.activeTurnStartedAt) return "Active now";
  const lastActive = formatAgo(activity?.lastFinishedAt, nowMs);
  if (lastActive) return `Last active ${lastActive}`;
  const started = formatAgo(session.createdAt, nowMs);
  return started ? `Started ${started}` : null;
}

const UNFINISHED_MESSAGE_STATUSES = new Set(["pending", "streaming"]);

/**
 * The transcript holds the result's finished output: at least one assistant
 * message of that attempt, none still streaming. A result without output
 * (an attempt that recorded none, or a turn that ended without an attempt)
 * has nothing more to display.
 */
export function transcriptShowsResult(
  messages: readonly ChatMessageDTO[],
  result: ChatSessionResultDTO,
): boolean {
  if (!result.hasOutput || result.attempt == null) return true;
  const output = messages.filter(
    (message) =>
      message.role === "assistant" &&
      message.taskId === result.taskId &&
      message.attempt === result.attempt,
  );
  return (
    output.length > 0 &&
    output.every((message) => !UNFINISHED_MESSAGE_STATUSES.has(message.status))
  );
}
