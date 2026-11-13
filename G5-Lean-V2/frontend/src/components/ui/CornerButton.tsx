// =============================================================================
// G5 Design System — CornerButton
// =============================================================================
// A floating secondary round button pinned to a bottom corner of the viewport,
// portaled to the body above page content and the modal tier. Two consumers
// share it so the corners stay identical: the platform report trigger on the
// left (platform_reports g5.quality.platform-reports.trigger) and the
// workspace Chat button on the right (workspaces
// g5.workspace.ui.chat-corner-button). A disabled button stays visible with
// its reason as the tooltip and acts on nothing.
//
// A `movable` button can be dragged anywhere in the viewport when it hides
// something underneath. The position is remembered per browser as offsets
// from its home corner, so a viewport resize keeps it against the same edges,
// and is clamped to the viewport on every read. A press that travels less
// than the drag threshold is still a click; a drag never clicks. The stack
// above the button is one handle with it: a drag that starts on any bubble
// moves button and stack together, and the lone stack of the center surface
// drags the same way, to the same remembered spot.
// =============================================================================

import {
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

import { classNames } from "./classNames";

export interface CornerButtonProps {
  corner: "left" | "right";
  /** Accessible name; also the tooltip while enabled. */
  label: string;
  /** Tooltip while enabled; defaults to `label`. */
  title?: string;
  /** Why the button cannot act; shown as the tooltip while disabled. */
  disabledReason?: string | null;
  onClick: () => void;
  testId?: string;
  /**
   * Let the button be dragged out of the way, even while disabled. Its
   * offset from the home corner is stored under `storageKey` in this browser.
   */
  movable?: { storageKey: string };
  /**
   * Content stacked above the button (the chat bubble stack, ui
   * g5.workspace.ui.chat-bubble-stack), kept at the same offset so it
   * follows a drag and itself a drag handle for the pair. Rendered as a
   * sibling: a button cannot nest buttons.
   */
  stack?: ReactNode;
  children: ReactNode;
}

/** Distance from the home corner's edges, in CSS pixels. */
interface CornerButtonOffset {
  x: number;
  y: number;
}

/** Mirrors the `h-10 w-10` classes below. */
const CORNER_BUTTON_SIZE = 40;
const CORNER_BUTTON_HOME_OFFSET: CornerButtonOffset = { x: 16, y: 16 };
/** Space between the button and the stack above it. */
const CORNER_BUTTON_STACK_GAP = 8;

/**
 * Where a stack sits for a button at `offset`: above the button's spot while
 * the spot is in the lower half of the viewport, else hanging below it, so
 * the stack stays inside the viewport wherever the button was dragged.
 * `buttonShown` leaves room for the button itself; without one the stack
 * starts at the spot.
 */
function cornerStackPlacement(
  corner: "left" | "right",
  offset: CornerButtonOffset,
  buttonShown: boolean,
): { style: CSSProperties; direction: "up" | "down" } {
  const horizontal =
    corner === "left" ? { left: offset.x } : { right: offset.x };
  const buttonTop = window.innerHeight - offset.y - CORNER_BUTTON_SIZE;
  if (buttonTop < window.innerHeight / 2) {
    return {
      direction: "down",
      style: {
        ...horizontal,
        top: buttonShown
          ? buttonTop + CORNER_BUTTON_SIZE + CORNER_BUTTON_STACK_GAP
          : buttonTop,
      },
    };
  }
  return {
    direction: "up",
    style: {
      ...horizontal,
      bottom: buttonShown
        ? offset.y + CORNER_BUTTON_SIZE + CORNER_BUTTON_STACK_GAP
        : offset.y,
    },
  };
}
/** A press that travels less than this stays a click. */
const CORNER_BUTTON_DRAG_THRESHOLD = 4;

function clampCornerButtonOffset(
  offset: CornerButtonOffset,
): CornerButtonOffset {
  const maxX = Math.max(0, window.innerWidth - CORNER_BUTTON_SIZE);
  const maxY = Math.max(0, window.innerHeight - CORNER_BUTTON_SIZE);
  return {
    x: Math.round(Math.max(0, Math.min(maxX, offset.x))),
    y: Math.round(Math.max(0, Math.min(maxY, offset.y))),
  };
}

function readStoredCornerButtonOffset(
  storageKey: string,
): CornerButtonOffset | null {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<CornerButtonOffset> | null;
    if (
      !parsed ||
      typeof parsed.x !== "number" ||
      typeof parsed.y !== "number" ||
      !Number.isFinite(parsed.x) ||
      !Number.isFinite(parsed.y)
    ) {
      return null;
    }
    return { x: parsed.x, y: parsed.y };
  } catch {
    return null;
  }
}

function writeStoredCornerButtonOffset(
  storageKey: string,
  offset: CornerButtonOffset,
): void {
  try {
    localStorage.setItem(storageKey, JSON.stringify(offset));
  } catch {
    // Per-browser bookkeeping must never break the button.
  }
}

/** The pointer handlers that turn an element into a drag handle. */
interface CornerDragHandlers {
  onPointerDown: (event: ReactPointerEvent<HTMLElement>) => void;
  onPointerMove: (event: ReactPointerEvent<HTMLElement>) => void;
  onPointerUp: (event: ReactPointerEvent<HTMLElement>) => void;
  onPointerCancel: (event: ReactPointerEvent<HTMLElement>) => void;
  /**
   * Swallows the click the browser dispatches after a drag before it
   * reaches the pressed element, whichever button under the handle it was.
   */
  onClickCapture: (event: ReactMouseEvent<HTMLElement>) => void;
}

/**
 * The remembered offset of a corner spot and the handlers that drag it. The
 * offset is read on mount and clamped to the viewport, so a position saved
 * on a larger screen still lands inside this one, and follows a viewport
 * that shrinks. Without a `storageKey` nothing moves: the offset is the home
 * corner and `handlers` is undefined.
 */
function useCornerDrag(
  corner: "left" | "right",
  storageKey: string | null,
): { offset: CornerButtonOffset; handlers: CornerDragHandlers | undefined } {
  const [offset, setOffset] = useState<CornerButtonOffset>(() =>
    clampCornerButtonOffset(
      (storageKey && readStoredCornerButtonOffset(storageKey)) ||
        CORNER_BUTTON_HOME_OFFSET,
    ),
  );
  useEffect(() => {
    if (!storageKey) return;
    const onResize = () =>
      setOffset((current) => clampCornerButtonOffset(current));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [storageKey]);

  // One press at a time: where it started, the offset it started from, the
  // offset it has reached and whether it has travelled far enough to count
  // as a drag. A drag ends with a click event the browser still dispatches,
  // which must not act.
  const pressRef = useRef<{
    pointerId: number;
    x: number;
    y: number;
    from: CornerButtonOffset;
    latest: CornerButtonOffset;
    dragging: boolean;
    /** The element holding pointer capture for this press. */
    captured: Element;
  } | null>(null);
  const suppressClickRef = useRef(false);

  const onPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 0 || pressRef.current) return;
      // A fresh press clears any suppression a cancelled drag left behind.
      suppressClickRef.current = false;
      // Capture at once, or a brisk drag leaves the handle before the
      // threshold and the moves never reach it. Capture the pressed element
      // itself, not the stack: a browser retargets the pointerup and the
      // click to the capturing element, and the click of a short press must
      // still land on the pressed bubble. jsdom has no pointer capture.
      const captured =
        event.target instanceof Element ? event.target : event.currentTarget;
      captured.setPointerCapture?.(event.pointerId);
      pressRef.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        from: offset,
        latest: offset,
        dragging: false,
        captured,
      };
    },
    [offset],
  );

  const onPointerMove = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const press = pressRef.current;
      if (!press || press.pointerId !== event.pointerId) return;
      const dx = event.clientX - press.x;
      const dy = event.clientY - press.y;
      if (!press.dragging) {
        if (Math.hypot(dx, dy) < CORNER_BUTTON_DRAG_THRESHOLD) return;
        press.dragging = true;
      }
      // Offsets grow away from the home corner: moving the pointer towards
      // the corner shrinks them.
      press.latest = clampCornerButtonOffset({
        x: corner === "right" ? press.from.x - dx : press.from.x + dx,
        y: press.from.y - dy,
      });
      setOffset(press.latest);
    },
    [corner],
  );

  const endPress = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const press = pressRef.current;
      if (!press || press.pointerId !== event.pointerId) return;
      pressRef.current = null;
      if (press.captured.hasPointerCapture?.(event.pointerId)) {
        press.captured.releasePointerCapture(event.pointerId);
      }
      if (!press.dragging || !storageKey) return;
      // Only a completed press is followed by a click, dispatched before
      // the next task; a touch drag or a cancelled press produces none, so
      // the suppression disarms right after rather than swallowing a later
      // keyboard activation.
      if (event.type === "pointerup") {
        suppressClickRef.current = true;
        setTimeout(() => {
          suppressClickRef.current = false;
        }, 0);
      }
      writeStoredCornerButtonOffset(storageKey, press.latest);
    },
    [storageKey],
  );

  // Stopping the click in the capture phase keeps it from every handler
  // below the handle, the pressed bubble's or the button's own.
  const onClickCapture = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    if (!suppressClickRef.current) return;
    suppressClickRef.current = false;
    event.stopPropagation();
    event.preventDefault();
  }, []);

  const handlers = useMemo<CornerDragHandlers | undefined>(
    () =>
      storageKey
        ? {
            onPointerDown,
            onPointerMove,
            onPointerUp: endPress,
            onPointerCancel: endPress,
            onClickCapture,
          }
        : undefined,
    [storageKey, onPointerDown, onPointerMove, endPress, onClickCapture],
  );

  return { offset, handlers };
}

export function CornerButton({
  corner,
  label,
  title,
  disabledReason = null,
  onClick,
  testId,
  movable,
  stack,
  children,
}: CornerButtonProps) {
  const disabled = disabledReason !== null;
  const storageKey = movable?.storageKey ?? null;
  const { offset, handlers } = useCornerDrag(corner, storageKey);

  const handleClick = useCallback(() => {
    if (!disabled) onClick();
  }, [disabled, onClick]);

  const position = storageKey
    ? {
        bottom: offset.y,
        ...(corner === "left" ? { left: offset.x } : { right: offset.x }),
        touchAction: "none" as const,
      }
    : undefined;
  const stackPlacement = cornerStackPlacement(corner, offset, true);

  return createPortal(
    <>
      {stack != null && (
        <div
          data-testid={testId ? `${testId}-stack` : undefined}
          data-screenshot-exclude=""
          data-stack-direction={stackPlacement.direction}
          className="fixed z-[120] flex w-10 flex-col items-center"
          style={{
            ...stackPlacement.style,
            ...(handlers && { touchAction: "none" as const }),
          }}
          {...handlers}
        >
          {stack}
        </div>
      )}
      <button
        type="button"
        data-testid={testId}
        // Kept out of a platform-report capture by node filter, not by hiding,
        // so the report drawer can open without the button flickering.
        data-screenshot-exclude=""
        aria-label={label}
        aria-disabled={disabled || undefined}
        title={disabled ? (disabledReason ?? undefined) : (title ?? label)}
        onClick={handleClick}
        {...handlers}
        className={classNames(
          "fixed z-[120] inline-flex h-10 w-10 items-center justify-center rounded-full border shadow-md transition-colors",
          !storageKey && "bottom-4",
          !storageKey && (corner === "left" ? "left-4" : "right-4"),
          disabled
            ? "cursor-not-allowed opacity-50"
            : "cursor-pointer hover:text-[var(--g5-text0)]",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
        )}
        style={{
          borderColor: "var(--g5-stroke)",
          background: "var(--g5-bg1)",
          color: "var(--g5-text1)",
          ...position,
        }}
      >
        {children}
      </button>
    </>,
    document.body,
  );
}

export interface CornerStackProps {
  corner: "left" | "right";
  /**
   * The movable button's storage key, so the stack sits where it was left
   * and a drag from any of its bubbles moves it, and the button with it,
   * to a new remembered spot.
   */
  storageKey?: string;
  testId?: string;
  children: ReactNode;
}

/**
 * The stack alone, at the corner button's spot, for the surfaces that show
 * no button (Chat as the center surface, ui g5.workspace.ui.chat-bubble-stack).
 * Shares the button's remembered offset: a drag here moves both.
 */
export function CornerStack({
  corner,
  storageKey,
  testId,
  children,
}: CornerStackProps) {
  const { offset, handlers } = useCornerDrag(corner, storageKey ?? null);
  const placement = cornerStackPlacement(corner, offset, false);
  return createPortal(
    <div
      data-testid={testId}
      data-screenshot-exclude=""
      data-stack-direction={placement.direction}
      className="fixed z-[120] flex w-10 flex-col items-center"
      style={{
        ...placement.style,
        ...(handlers && { touchAction: "none" as const }),
      }}
      {...handlers}
    >
      {children}
    </div>,
    document.body,
  );
}
