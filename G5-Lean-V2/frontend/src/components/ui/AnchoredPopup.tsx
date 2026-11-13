// =============================================================================
// G5 Design System — anchored popup
// =============================================================================
// A small dialog that opens next to the control that opened it (approval
// commit evidence and requirement descriptions, g5.governance.approval.evidence)
// instead of in the viewport centre. It shares the Modal dismissal contract:
// × at the top right, a press that starts and ends outside closes, Escape
// closes, and useDialogFocus owns initial focus, Tab trapping and focus
// restore. Placement prefers below the anchor, flips above it, and is clamped
// to the viewport; long content scrolls inside the panel.
// =============================================================================

import React, { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { FLOATING_SURFACE_ATTR } from "../../lib/floatingSurface";
import {
  placeAnchoredPopup,
  POPUP_VIEWPORT_MARGIN,
  type PopupPlacement,
} from "../../lib/popupPlacement";
import { classNames } from "./classNames";
import { MODAL_DIALOG_PRIORITY, useDialogFocus } from "./useDialogFocus";

const MARGIN = POPUP_VIEWPORT_MARGIN;

export interface AnchoredPopupProps {
  /** The control that opened the popup; placement follows it. */
  anchor: HTMLElement | null;
  title: React.ReactNode;
  ariaLabel: string;
  onClose: () => void;
  /** Panel width classes; defaults to a reading width capped by the viewport. */
  widthClass?: string;
  testId?: string;
  children: React.ReactNode;
}

export function AnchoredPopup({
  anchor,
  title,
  ariaLabel,
  onClose,
  widthClass = "w-[min(40rem,calc(100vw-16px))]",
  testId,
  children,
}: AnchoredPopupProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const pressStartedOutside = useRef(false);
  const [placement, setPlacement] = useState<PopupPlacement | null>(null);

  useDialogFocus(panelRef, { onClose, priority: MODAL_DIALOG_PRIORITY });

  useLayoutEffect(() => {
    const place = () => {
      const panel = panelRef.current;
      if (!panel) return;
      const rect = anchor?.isConnected ? anchor.getBoundingClientRect() : null;
      const box = panel.getBoundingClientRect();
      const next = placeAnchoredPopup(
        rect,
        { width: box.width, height: box.height },
        { width: window.innerWidth, height: window.innerHeight },
      );
      setPlacement((current) =>
        current && current.top === next.top && current.left === next.left
          ? current
          : next,
      );
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    if (panelRef.current) observer?.observe(panelRef.current);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
      observer?.disconnect();
    };
  }, [anchor]);

  return createPortal(
    <div
      data-testid={testId ? `${testId}-overlay` : "anchored-popup-overlay"}
      {...{ [FLOATING_SURFACE_ATTR]: "" }}
      className="fixed inset-0 z-50 bg-black/30"
      onMouseDown={(event) => {
        pressStartedOutside.current = event.target === event.currentTarget;
      }}
      onMouseUp={(event) => {
        const startedOutside = pressStartedOutside.current;
        pressStartedOutside.current = false;
        if (startedOutside && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel}
        tabIndex={-1}
        data-testid={testId}
        style={{
          top: placement?.top ?? MARGIN,
          left: placement?.left ?? MARGIN,
          maxHeight: `calc(100vh - ${2 * MARGIN}px)`,
        }}
        className={classNames(
          "fixed flex flex-col",
          widthClass,
          "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
          "bg-[var(--g5-bg0)] text-[var(--g5-text0)] shadow-xl",
        )}
      >
        <div className="flex items-start gap-3 border-b border-[var(--g5-stroke)] px-4 py-3">
          <h2 className="min-w-0 flex-1 text-base font-semibold">{title}</h2>
          <button
            type="button"
            aria-label="Close"
            data-testid={testId ? `${testId}-close` : "anchored-popup-close"}
            onClick={onClose}
            className="shrink-0 rounded p-1 text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto px-4 py-3 text-sm">
          {children}
        </div>
      </div>
    </div>,
    document.body,
  );
}
