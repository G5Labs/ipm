// =============================================================================
// G5 Client — dismiss a menu on an outside click or Escape
// =============================================================================
// The one dismissal rule every dropdown follows (the ribbon's menus, the
// ribbon's owner menu): while `open`, a mousedown outside `ref` or an Escape
// keypress calls `onDismiss`. Listeners attach only while open.
// =============================================================================

import { useEffect, type RefObject } from "react";

export function useDismissOnOutside(
  ref: RefObject<HTMLElement | null>,
  open: boolean,
  onDismiss: () => void,
): void {
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) onDismiss();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onDismiss();
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [ref, open, onDismiss]);
}
