import type { ReactNode } from "react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { classNames } from "../ui/UI";

const DEFAULT_WIDTH_PX = 416;
const DEFAULT_EDGE_PADDING_PX = 8;
const DEFAULT_ANCHOR_GAP_PX = 4;

interface AnchoredPopoverProps {
  anchor: HTMLElement;
  ariaLabel: string;
  children: ReactNode;
  onDismiss: () => void;
  testId?: string;
  width?: number;
  edgePadding?: number;
  anchorGap?: number;
  className?: string;
}

/**
 * Shared interaction shell for compact content anchored to a DOM element.
 *
 * The popover is portaled to the document body and positioned relative to the
 * viewport so editor overflow and stacking contexts cannot clip it.
 */
export const AnchoredPopover = ({
  anchor,
  ariaLabel,
  children,
  onDismiss,
  testId,
  width = DEFAULT_WIDTH_PX,
  edgePadding = DEFAULT_EDGE_PADDING_PX,
  anchorGap = DEFAULT_ANCHOR_GAP_PX,
  className,
}: AnchoredPopoverProps) => {
  const popoverRef = useRef<HTMLElement | null>(null);
  const computePosition = useCallback(
    (popoverHeight: number) => {
      const rect = anchor.getBoundingClientRect();
      const maxLeft = Math.max(
        edgePadding,
        window.innerWidth - width - edgePadding,
      );
      const maxTop = Math.max(
        edgePadding,
        window.innerHeight - popoverHeight - edgePadding,
      );
      const belowTop = rect.bottom + anchorGap;
      const aboveTop = rect.top - anchorGap - popoverHeight;
      const top =
        popoverHeight > 0 &&
        belowTop + popoverHeight > window.innerHeight - edgePadding &&
        aboveTop >= edgePadding
          ? aboveTop
          : Math.min(Math.max(belowTop, edgePadding), maxTop);
      return {
        top,
        left: Math.min(Math.max(rect.left, edgePadding), maxLeft),
      };
    },
    [anchor, anchorGap, edgePadding, width],
  );
  const [position, setPosition] = useState(() => computePosition(0));

  useLayoutEffect(() => {
    const updatePosition = () => {
      if (!anchor.isConnected) {
        onDismiss();
        return;
      }
      setPosition(
        computePosition(
          popoverRef.current?.getBoundingClientRect().height ?? 0,
        ),
      );
    };
    updatePosition();
    window.addEventListener("scroll", updatePosition, true);
    window.addEventListener("resize", updatePosition);
    return () => {
      window.removeEventListener("scroll", updatePosition, true);
      window.removeEventListener("resize", updatePosition);
    };
  }, [anchor, computePosition, onDismiss]);

  useEffect(() => {
    const dismissWhenAnchorIsRemoved = new MutationObserver(() => {
      if (!anchor.isConnected) onDismiss();
    });
    dismissWhenAnchorIsRemoved.observe(document.body, {
      childList: true,
      subtree: true,
    });
    return () => dismissWhenAnchorIsRemoved.disconnect();
  }, [anchor, onDismiss]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const eventTarget = event.target as Node | null;
      if (!eventTarget) return;
      if (popoverRef.current?.contains(eventTarget)) return;
      if (anchor.contains(eventTarget)) return;
      if (
        eventTarget instanceof Element &&
        eventTarget.closest("[data-g5-anchored-popover-trigger]")
      ) {
        return;
      }
      onDismiss();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onDismiss();
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [anchor, onDismiss]);

  return createPortal(
    <section
      ref={popoverRef}
      role="dialog"
      aria-label={ariaLabel}
      data-testid={testId}
      style={{
        position: "fixed",
        top: position.top,
        left: position.left,
        width,
        maxWidth: `calc(100vw - ${edgePadding * 2}px)`,
        maxHeight: `calc(100vh - ${edgePadding * 2}px)`,
        overflowY: "auto",
        zIndex: 1000,
      }}
      className={classNames(
        "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-2 shadow-lg",
        className,
      )}
    >
      {children}
    </section>,
    document.body,
  );
};
