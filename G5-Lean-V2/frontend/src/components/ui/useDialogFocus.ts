// =============================================================================
// G5 Design System — useDialogFocus
// =============================================================================
// The single implementation of dialog keyboard and focus lifecycle
// (design-system D15): Escape-to-close (suspended by closeDisabled), Tab
// focus trap over the panel's visible enabled focusables, initial focus, and
// focus restore to the previously focused element on unmount. Reusable by
// non-Modal overlay surfaces that need the same lifecycle without the
// centered shell.
// =============================================================================

import { useEffect, useRef, useSyncExternalStore } from "react";

const FOCUSABLE_SELECTOR =
  'button:not([disabled]), input:not([disabled]), [href], select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface DialogStackEntry {
  ref: React.RefObject<HTMLElement | null>;
  /** Overlay z-priority — mirrors the overlay's z-index tier. */
  priority: number;
  /** Mount recency; tie-breaker among equal priorities. */
  seq: number;
}

// Registry of mounted dialog panels. Every hooked overlay installs a document
// keydown listener; only the VISUALLY top-most one may act on Escape/Tab.
// Mount order alone is not enough: overlays carry different z tiers (the
// issue-report drawer sits above modals, which are z-50/z-[60]), so a
// modal that mounts under an already-open higher-z overlay must not steal
// its keyboard handling. Top-most = highest priority, then most recent mount.
const dialogStack: DialogStackEntry[] = [];
let dialogSeqCounter = 0;

/** The modal overlay tier (`Modal` default); lower tiers are flyouts. */
export const MODAL_DIALOG_PRIORITY = 50;
/**
 * The platform report drawer's keyboard tier (`PlatformIssueReportButton`).
 * Tiers are ordinal: the drawer's CSS layer is higher than this number.
 */
export const REPORT_DRAWER_PRIORITY = 130;

// Subscribers to stack membership changes, for surfaces that must react to
// an overlay covering the page chrome (platform_reports
// g5.quality.platform-reports.trigger, workspaces
// g5.workspace.ui.chat-corner-button).
const stackListeners = new Set<() => void>();
let stackVersion = 0;

function notifyStackChanged(): void {
  stackVersion += 1;
  for (const listener of stackListeners) listener();
}

function subscribeStack(listener: () => void): () => void {
  stackListeners.add(listener);
  return () => {
    stackListeners.delete(listener);
  };
}

function getStackVersion(): number {
  return stackVersion;
}

/**
 * Whether an overlay that covers the page chrome is mounted: one registered at
 * the modal tier or above, below the report drawer's own tier. The settings
 * flyout sits under the modal tier and does not count; the report drawer is
 * excluded so a trigger shown for a modal stays while the drawer opens over
 * it and can take focus back on close.
 */
export function coveringOverlayOpen(): boolean {
  return dialogStack.some(
    (entry) =>
      entry.priority >= MODAL_DIALOG_PRIORITY &&
      entry.priority < REPORT_DRAWER_PRIORITY,
  );
}

/** Reactive form of `coveringOverlayOpen`. */
export function useCoveringOverlayOpen(): boolean {
  useSyncExternalStore(subscribeStack, getStackVersion, getStackVersion);
  return coveringOverlayOpen();
}

function topOfStack(): DialogStackEntry | undefined {
  let top: DialogStackEntry | undefined;
  for (const entry of dialogStack) {
    if (
      !top ||
      entry.priority > top.priority ||
      (entry.priority === top.priority && entry.seq > top.seq)
    ) {
      top = entry;
    }
  }
  return top;
}

// Openers saved by dialogs that closed WITHOUT restoring focus (a non-LIFO
// close, or a simultaneous whole-stack teardown where the lower dialog's
// cleanup runs first). The next top-most restore consumes the nearest
// connected one when its own saved opener has been disconnected.
const pendingOpeners: HTMLElement[] = [];

export interface UseDialogFocusOptions {
  onClose: () => void;
  /** Suspends Escape while a mutation is in flight. */
  closeDisabled?: boolean;
  /** Element to receive initial focus; defaults to the first focusable, else the panel. */
  initialFocusRef?: React.RefObject<HTMLElement | null>;
  /**
   * Overlay z-priority, mirroring the overlay's z-index tier (Modal passes
   * 50/60; the issue-report drawer 130). Determines which overlay is
   * "top-most" for Escape/Tab and focus restore when overlays with different
   * tiers coexist. Defaults to 50 (the modal tier).
   */
  priority?: number;
  /**
   * When false the hook is inert: no stack entry, no listeners, no focus
   * capture. For always-mounted surfaces that merely hide their panel (e.g.
   * a slide-out flyout), pass the panel's open state here so the lifecycle
   * runs per open/close cycle rather than per mount.
   */
  enabled?: boolean;
}

function focusableElements(panel: HTMLElement): HTMLElement[] {
  return Array.from(
    panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
  ).filter((element) => {
    // tabindex="-1" removes an element from the tab order even when another
    // selector branch (e.g. [href]) matched it.
    if (element.tabIndex < 0) return false;
    if (element.hidden) return false;
    // Covers both the element's own aria-hidden and any hidden ancestor.
    if (element.closest('[aria-hidden="true"]')) return false;
    // Rendered visibility (display/visibility/content-visibility).
    // checkVisibility is unavailable in jsdom, where layout doesn't exist —
    // treat elements as visible there.
    if (
      typeof element.checkVisibility === "function" &&
      !element.checkVisibility()
    ) {
      return false;
    }
    return true;
  });
}

export function useDialogFocus(
  panelRef: React.RefObject<HTMLElement | null>,
  {
    onClose,
    closeDisabled = false,
    initialFocusRef,
    priority = 50,
    enabled = true,
  }: UseDialogFocusOptions,
): void {
  // Keep the latest callbacks/flags in refs so the document listener and the
  // restore effect are installed exactly once per activation.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const closeDisabledRef = useRef(closeDisabled);
  closeDisabledRef.current = closeDisabled;
  const priorityRef = useRef(priority);
  priorityRef.current = priority;
  // Whether this dialog was the top of the stack when it closed. Captured by
  // the stack effect's cleanup (which runs first, before the entry is
  // removed) and consumed by the restore cleanup below, so a dialog closing
  // UNDER a still-open one never yanks focus behind the surviving modal.
  const wasTopMostAtCloseRef = useRef<boolean | null>(null);

  useEffect(() => {
    if (!enabled) return;
    const entry: DialogStackEntry = {
      ref: panelRef,
      priority: priorityRef.current,
      seq: ++dialogSeqCounter,
    };
    dialogStack.push(entry);
    notifyStackChanged();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (topOfStack() !== entry) return;
      if (event.key === "Escape") {
        if (!closeDisabledRef.current) {
          event.stopPropagation();
          onCloseRef.current();
        }
        return;
      }
      if (event.key !== "Tab") return;

      const panel = panelRef.current;
      if (!panel) return;
      const focusable = focusableElements(panel);
      if (focusable.length === 0) {
        event.preventDefault();
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement;
      if (!panel.contains(active)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      } else if (event.shiftKey && active === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      wasTopMostAtCloseRef.current = topOfStack() === entry;
      const index = dialogStack.indexOf(entry);
      if (index !== -1) dialogStack.splice(index, 1);
      notifyStackChanged();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;
    const previouslyFocused =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

    // Initial focus belongs only to the VISUALLY top-most overlay: a
    // lower-priority dialog mounting under an already-open higher-priority
    // overlay (e.g. a z-50 modal under the z-[130] drawer) must not pull
    // focus behind it. (The stack effect runs first, so this entry is
    // already registered when we ask.)
    if (topOfStack()?.ref === panelRef) {
      const panel = panelRef.current;
      const initial = initialFocusRef?.current;
      if (initial) {
        initial.focus();
      } else if (panel && !panel.contains(document.activeElement)) {
        // Respect focus already placed inside the panel (e.g. an autoFocus
        // input, which React applies before effects run).
        const focusable = focusableElements(panel);
        (focusable[0] ?? panel).focus();
      }
    }

    return () => {
      const wasTopMost =
        wasTopMostAtCloseRef.current ?? topOfStack()?.ref === panelRef;
      wasTopMostAtCloseRef.current = null;
      if (!wasTopMost) {
        // A dialog closed UNDER a still-open one (non-LIFO close, or the
        // lower dialog of a simultaneous whole-stack teardown). Do not move
        // focus — but keep the saved opener available: if the dialog above
        // is also closing, its own saved target sits inside THIS dialog's
        // now-disconnected subtree, and this opener is its way back out.
        if (previouslyFocused) pendingOpeners.push(previouslyFocused);
        return;
      }
      // Top-most close: restore to this dialog's saved opener, falling back
      // through openers handed up by dialogs that closed beneath us.
      const candidates = [previouslyFocused, ...[...pendingOpeners].reverse()];
      const target = candidates.find((element) => element?.isConnected) ?? null;
      for (let i = pendingOpeners.length - 1; i >= 0; i--) {
        if (pendingOpeners[i] === target || !pendingOpeners[i].isConnected) {
          pendingOpeners.splice(i, 1);
        }
      }
      if (dialogStack.length === 0) pendingOpeners.length = 0;
      const survivingTop = topOfStack()?.ref.current;
      if (target && (!survivingTop || survivingTop.contains(target))) {
        target.focus();
        return;
      }
      if (survivingTop) {
        // Either no connected opener, or the opener lies OUTSIDE the
        // overlay that is now top-most (e.g. the drawer closes over a modal
        // that mounted beneath it) — focusing it would put focus behind
        // that overlay. Hand focus to the surviving top instead and keep
        // the opener for when the stack empties.
        if (target) pendingOpeners.push(target);
        const focusable = focusableElements(survivingTop);
        (focusable[0] ?? survivingTop).focus();
        return;
      }
      if (target) target.focus();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);
}
