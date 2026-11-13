// =============================================================================
// G5 Design System — Modal shell
// =============================================================================
// The one dialog implementation every centered blocking dialog renders
// through (design-system D14). Owns: portal to document.body, the single
// overlay treatment (bg-black/50, px-4 responsive floor), the two z tiers,
// the token-styled panel with a named size map, Escape/click-outside
// dismissal gated by closeDisabled, dialog ARIA, and — via useDialogFocus —
// initial focus, Tab trapping, and focus restore.
// =============================================================================

import React, { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { FLOATING_SURFACE_ATTR } from "../../lib/floatingSurface";
import { classNames } from "./classNames";
import { useDialogFocus } from "./useDialogFocus";

// Count of open Modals holding the body scroll lock: nested/stacked modals
// each take the lock, and the body only scrolls again when all release it.
let scrollLockCount = 0;
let scrollLockPreviousOverflow = "";

export type ModalSize = "sm" | "md" | "lg" | "xl";
export type ModalTier = "modal" | "top";

const SIZE_CLASS: Record<ModalSize, string> = {
  sm: "max-w-md",
  md: "max-w-xl",
  lg: "max-w-3xl",
  xl: "max-w-5xl",
};

const TIER_CLASS: Record<ModalTier, string> = {
  modal: "z-50",
  top: "z-[60]",
};

/** Exactly one accessible-name prop is required: a visible title's id, or a label. */
type ModalAccessibleName =
  | {
      /** id of the visible title element (preferred when a title is rendered). */
      labelledBy: string;
      ariaLabel?: never;
    }
  | {
      labelledBy?: never;
      /** Accessible name when there is no visible title element. */
      ariaLabel: string;
    };

export type ModalProps = ModalAccessibleName & {
  onClose: () => void;
  /** Panel max-width: sm=max-w-md, md=max-w-xl (default), lg=max-w-3xl, xl=max-w-5xl. */
  size?: ModalSize;
  /** "top" stacks above another open modal (confirm-inside-wizard). */
  tier?: ModalTier;
  /** Suspends Escape and overlay dismissal while a mutation is in flight. */
  closeDisabled?: boolean;
  /** Element to receive initial focus; defaults to the first focusable. */
  initialFocusRef?: React.RefObject<HTMLElement | null>;
  /**
   * Extra classes for the panel (padding defaults to p-4; pass p-0 layouts
   * here). NOTE for `flex flex-col` layouts with an internal scroll region:
   * the scrolling child needs `min-h-0` — flex items default to
   * `min-height: auto` and never shrink below content height, so without it
   * the whole panel scrolls and carries header/footer out of view.
   */
  className?: string;
  testId?: string;
  children: React.ReactNode;
};

/**
 * Shared modal shell. Renders the overlay + centered panel through a portal
 * into document.body so no ancestor overflow/transform/stacking context can
 * clip or bury a dialog. Dismissal contract: Escape closes; a click that both
 * starts (pointer-down) and ends on the overlay closes; clicks and drags that
 * start inside the panel never close. `closeDisabled` suspends both.
 */
export const Modal: React.FC<ModalProps> = ({
  onClose,
  labelledBy,
  ariaLabel,
  size = "md",
  tier = "modal",
  closeDisabled = false,
  initialFocusRef,
  className,
  testId,
  children,
}) => {
  const panelRef = useRef<HTMLDivElement>(null);
  // Overlay dismissal requires the press to both START and RELEASE on the
  // overlay itself. A click-based check is not enough: when a drag crosses
  // the overlay/panel boundary the browser dispatches the click to their
  // common ancestor (the overlay), which would pass a target check even
  // though one end of the gesture was inside the panel.
  const pressStartedOnOverlayRef = useRef(false);

  useDialogFocus(panelRef, {
    onClose,
    closeDisabled,
    initialFocusRef,
    // Mirror the overlay z tier so keyboard top-most tracks visual top-most
    // when overlays with different tiers coexist.
    priority: tier === "top" ? 60 : 50,
  });

  // Lock background scroll while any modal is open: the overlay is
  // fixed inset-0, so wheel/trackpad gestures over it would otherwise
  // scroll the page behind the dialog.
  useEffect(() => {
    if (scrollLockCount === 0) {
      scrollLockPreviousOverflow = document.body.style.overflow;
      document.body.style.overflow = "hidden";
    }
    scrollLockCount += 1;
    return () => {
      scrollLockCount -= 1;
      if (scrollLockCount === 0) {
        document.body.style.overflow = scrollLockPreviousOverflow;
      }
    };
  }, []);

  return createPortal(
    <div
      data-testid={testId ? `${testId}-overlay` : "modal-overlay"}
      // A modal is a floating surface: the rail overlay that opened it (a
      // Guided preview control) stays while the pointer or a click is in it.
      {...{ [FLOATING_SURFACE_ATTR]: "" }}
      className={classNames(
        "fixed inset-0 flex items-center justify-center bg-black/50 px-4",
        TIER_CLASS[tier],
      )}
      onMouseDown={(event) => {
        pressStartedOnOverlayRef.current = event.target === event.currentTarget;
      }}
      onMouseUp={(event) => {
        const startedOnOverlay = pressStartedOnOverlayRef.current;
        pressStartedOnOverlayRef.current = false;
        if (
          !closeDisabled &&
          startedOnOverlay &&
          event.target === event.currentTarget
        ) {
          onClose();
        }
      }}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-label={labelledBy ? undefined : ariaLabel}
        tabIndex={-1}
        data-testid={testId}
        className={classNames(
          "w-full max-h-[85vh] overflow-y-auto",
          SIZE_CLASS[size],
          "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
          // The portal parents the panel on <body>, outside #root, so the
          // panel sets the theme foreground with its background rather than
          // inheriting whatever color <body> carries.
          "bg-[var(--g5-bg0)] text-[var(--g5-text0)] shadow-xl",
          // className REPLACES the p-4 default (see the prop JSDoc: p-0
          // layouts own their padding; flex-col scroll children need min-h-0).
          className ?? "p-4",
        )}
      >
        {children}
      </div>
    </div>,
    document.body,
  );
};
