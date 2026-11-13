// =============================================================================
// G5 Chat — ChatBubbleStack
// =============================================================================
// One round bubble per chat of the workspace, stacked above the Chat corner
// button in the list's order, most recent nearest the button (ui
// g5.workspace.ui.chat-bubble-stack). The face says what the chat is doing —
// working, waiting for your answer, or idle, with a dot while its latest
// result is still unviewed — from the server's session activity
// (lib/chatSessionActivity.ts). Pressing a bubble asks the page to open that
// chat where the corner button opens Chat. The per-browser settings choose the
// faces (still images, alternating frames, or a robot-heads head) and how many
// bubbles show before the rest fold into a count; pressing the count unfolds
// every chat until it is folded back. Reduced motion stills the frames and
// holds the 3D pose.
// =============================================================================

import { lazy, Suspense, useEffect, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";

import questionA96 from "../../assets/chat-bubbles/question-a-96.png";
import questionA192 from "../../assets/chat-bubbles/question-a-192.png";
import questionB96 from "../../assets/chat-bubbles/question-b-96.png";
import questionB192 from "../../assets/chat-bubbles/question-b-192.png";
import sleepingA96 from "../../assets/chat-bubbles/sleeping-a-96.png";
import sleepingA192 from "../../assets/chat-bubbles/sleeping-a-192.png";
import sleepingB96 from "../../assets/chat-bubbles/sleeping-b-96.png";
import sleepingB192 from "../../assets/chat-bubbles/sleeping-b-192.png";
import workingA96 from "../../assets/chat-bubbles/working-a-96.png";
import workingA192 from "../../assets/chat-bubbles/working-a-192.png";
import workingB96 from "../../assets/chat-bubbles/working-b-96.png";
import workingB192 from "../../assets/chat-bubbles/working-b-192.png";
import {
  type ChatBubbleState,
  chatBubbleState,
} from "../../lib/chatSessionActivity";
import {
  CHAT_BUBBLE_LIMIT_DEFAULT,
  type ChatBubbleStyle,
  type ChatSessionDTO,
} from "../../lib/types";
import { classNames } from "../ui/classNames";

interface Face {
  x1: string;
  x2: string;
}

/** The still face of each state and the frame Animated alternates it with. */
const FACES: Record<ChatBubbleState, { still: Face; alt: Face }> = {
  working: {
    still: { x1: workingB96, x2: workingB192 },
    alt: { x1: workingA96, x2: workingA192 },
  },
  waiting: {
    still: { x1: questionB96, x2: questionB192 },
    alt: { x1: questionA96, x2: questionA192 },
  },
  idle: {
    still: { x1: sleepingA96, x2: sleepingA192 },
    alt: { x1: sleepingB96, x2: sleepingB192 },
  },
};

export const CHAT_BUBBLE_STATE_LABELS: Record<ChatBubbleState, string> = {
  waiting: "Waiting for your answer",
  working: "Working",
  idle: "Idle",
};
/** The idle face with a result the person has not viewed. */
export const CHAT_BUBBLE_IDLE_NEW_RESULT_LABEL = "Idle, new result";

/** robot-heads states: no question face exists, so waiting is `happy`. */
const ROBOT_HEAD_STATES: Record<
  ChatBubbleState,
  "working" | "happy" | "sleeping"
> = {
  working: "working",
  waiting: "happy",
  idle: "sleeping",
};

export interface ChatBubble {
  session: ChatSessionDTO;
  state: ChatBubbleState;
}

/**
 * One bubble per chat, in the list's order: the server lists chats by most
 * recent activity (chat g5.chat.activity.order), so the first is nearest the
 * button.
 */
export function chatBubbles(sessions: readonly ChatSessionDTO[]): ChatBubble[] {
  return sessions.map((session) => ({
    session,
    state: chatBubbleState(session.activity),
  }));
}

// The 3D head is a separate chunk, fetched only once the style asks for it.
// A chunk that fails to load leaves the face empty rather than reaching the
// nearest error boundary: the bubble's ring and label still say the state.
type RobotHeadComponent = (typeof import("robot-heads"))["RobotHead"];
const RobotHead = lazy<RobotHeadComponent>(() =>
  import("robot-heads")
    .then((module) => ({ default: module.RobotHead }))
    .catch(() => ({
      default: (() => null) as unknown as RobotHeadComponent,
    })),
);

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () => window.matchMedia?.(REDUCED_MOTION_QUERY)?.matches ?? false,
  );
  useEffect(() => {
    const query = window.matchMedia?.(REDUCED_MOTION_QUERY);
    if (!query?.addEventListener) return;
    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);
  return reduced;
}

/** A stable 0–1 seed per chat so a row of heads blinks out of step. */
function seedFor(id: string): number {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) {
    hash = (hash * 31 + id.charCodeAt(i)) % 1000;
  }
  return hash / 1000;
}

export interface ChatBubbleStackProps {
  /** The workspace's chats with their activity, in the list's order. */
  sessions: readonly ChatSessionDTO[];
  style: ChatBubbleStyle;
  /** Bubbles shown before the rest fold into a count; default 5. */
  limit?: number;
  /** Open this chat at the corner. */
  onOpen: (sessionId: string) => void;
}

export function ChatBubbleStack({
  sessions,
  style,
  limit = CHAT_BUBBLE_LIMIT_DEFAULT,
  onOpen,
}: ChatBubbleStackProps) {
  const reducedMotion = usePrefersReducedMotion();
  // Unfolded until folded back; a remount (the stack leaves while the
  // floating panel is open) starts folded again.
  const [expanded, setExpanded] = useState(false);
  if (style === "off") return null;
  const bubbles = chatBubbles(sessions);
  if (bubbles.length === 0) return null;
  const folded = bubbles.length - Math.max(1, Math.floor(limit));
  const shown = expanded || folded <= 0 ? bubbles : bubbles.slice(0, -folded);
  const animated = style === "animated" && !reducedMotion;

  return (
    <div
      data-testid="chat-bubble-stack"
      data-style={style}
      data-expanded={expanded || undefined}
      className="g5-chat-bubble-stack flex flex-col items-center gap-2"
    >
      {/* The count unfolds every chat; while unfolded, the same spot folds
          the rest away again. Both sit past the farthest bubble. */}
      {folded > 0 && (
        <button
          type="button"
          data-testid={expanded ? "chat-bubble-fewer" : "chat-bubble-more"}
          aria-label={
            expanded
              ? `Show the ${bubbles.length - folded} most recent ${bubbles.length - folded === 1 ? "chat" : "chats"} only`
              : `${folded} more ${folded === 1 ? "chat" : "chats"}; show all`
          }
          aria-expanded={expanded}
          title={expanded ? "Show fewer" : `${folded} more`}
          onClick={() => setExpanded((current) => !current)}
          className={classNames(
            "inline-flex items-center gap-0.5 rounded-full border px-2 py-0.5 text-[11px] font-semibold shadow-md",
            "border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text1)] hover:text-[var(--g5-text0)]",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          )}
        >
          {expanded ? (
            <ChevronDown className="h-3 w-3" aria-hidden="true" />
          ) : (
            <>
              +{folded}
              <ChevronUp className="h-3 w-3" aria-hidden="true" />
            </>
          )}
        </button>
      )}
      {/* The nearest bubble renders last so it sits just above the button. */}
      {shown
        .map((bubble) => {
          const { session, state } = bubble;
          const title = session.title?.trim() || "Untitled chat";
          const label =
            state === "idle" && session.activity?.unseenResult
              ? CHAT_BUBBLE_IDLE_NEW_RESULT_LABEL
              : CHAT_BUBBLE_STATE_LABELS[state];
          const face = FACES[state];
          return (
            <button
              key={session.id}
              type="button"
              data-testid="chat-bubble"
              data-session-id={session.id}
              data-state={state}
              aria-label={`${title}: ${label}`}
              onClick={() => onOpen(session.id)}
              className={classNames(
                "group/bubble relative inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-full border shadow-md transition-transform hover:scale-105 motion-reduce:transition-none",
                "border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                state === "working" && "g5-chat-bubble-working",
                state === "waiting" && "g5-chat-bubble-waiting",
              )}
            >
              <span
                aria-hidden="true"
                className={classNames(
                  "absolute inset-0 overflow-hidden rounded-full",
                  state === "idle" && "opacity-80",
                  style === "3d" && "g5-chat-bubble-3d",
                )}
              >
                {style === "3d" ? (
                  <Suspense fallback={null}>
                    <RobotHead
                      state={ROBOT_HEAD_STATES[state]}
                      size={40}
                      interactive={false}
                      antenna
                      floorShadow={false}
                      paused={reducedMotion}
                      seed={seedFor(session.id)}
                    />
                  </Suspense>
                ) : (
                  <>
                    <img
                      src={face.still.x1}
                      srcSet={`${face.still.x1} 1x, ${face.still.x2} 2x`}
                      alt=""
                      draggable={false}
                      className="g5-chat-bubble-face"
                    />
                    {animated && (
                      <img
                        src={face.alt.x1}
                        srcSet={`${face.alt.x1} 1x, ${face.alt.x2} 2x`}
                        alt=""
                        draggable={false}
                        data-testid="chat-bubble-frame"
                        data-period={state}
                        className="g5-chat-bubble-face g5-chat-bubble-frame-alt"
                      />
                    )}
                  </>
                )}
              </span>
              {session.activity?.unseenResult && (
                <span
                  data-testid="chat-bubble-unseen"
                  className="absolute -right-0.5 -top-0.5 h-2.5 w-2.5 rounded-full bg-[var(--g5-info)] ring-2 ring-[var(--g5-bg1)]"
                />
              )}
              {/* The label shows at once beside the bubble on hover or
                  keyboard focus; the accessible name carries the same. */}
              <span
                aria-hidden="true"
                data-testid="chat-bubble-label"
                className="pointer-events-none absolute right-[calc(100%+8px)] top-1/2 -translate-y-1/2 whitespace-nowrap rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs text-[var(--g5-text0)] opacity-0 shadow-md transition-opacity group-focus-visible/bubble:opacity-100 group-hover/bubble:opacity-100 motion-reduce:transition-none"
              >
                {title}
                <span className="ml-1.5 text-[var(--g5-text2)]">{label}</span>
              </span>
            </button>
          );
        })
        .reverse()}
    </div>
  );
}
