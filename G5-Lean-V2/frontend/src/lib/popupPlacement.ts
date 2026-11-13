// Viewport placement for popups anchored to the control that opened them
// (g5.governance.approval.evidence): below when it fits, else above, else as
// close as the viewport allows; never off screen.

export const POPUP_VIEWPORT_MARGIN = 8;
const MARGIN = POPUP_VIEWPORT_MARGIN;
const GAP = 6;

export interface PopupPlacement {
  top: number;
  left: number;
}

/**
 * Viewport placement for a panel of `size` next to `anchor`: below when it
 * fits, else above, else as close as the viewport allows; never off screen.
 */
export function placeAnchoredPopup(
  anchor: { top: number; bottom: number; left: number } | null,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
): PopupPlacement {
  const clamp = (value: number, max: number) =>
    Math.max(MARGIN, Math.min(value, Math.max(MARGIN, max)));
  const maxTop = viewport.height - MARGIN - size.height;
  const maxLeft = viewport.width - MARGIN - size.width;
  if (!anchor)
    return {
      top: clamp((viewport.height - size.height) / 2, maxTop),
      left: clamp((viewport.width - size.width) / 2, maxLeft),
    };
  const below = anchor.bottom + GAP;
  const above = anchor.top - GAP - size.height;
  const top =
    below <= maxTop ? below : above >= MARGIN ? above : clamp(below, maxTop);
  return { top, left: clamp(anchor.left, maxLeft) };
}
