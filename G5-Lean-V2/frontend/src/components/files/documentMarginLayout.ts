// =============================================================================
// documentMarginLayout — where each card sits in the margin beside a document
// =============================================================================
// A margin item is level with its text when it can be: items are placed in
// the order of their anchors, each at its anchor or below the item before it.
// The active item (last expanded, selected or reached from its mark) is set
// level with its text, the items before it pushed up, never above the
// margin's top, and the items after it pushed down. Items with no place follow
// the placed ones in the order given (`g5.documents.comments.identity`,
// `g5.documents.review-placement`).
// =============================================================================

export interface MarginItemMeasure {
  id: string;
  /** Top of the item's text relative to the margin; null when it has none. */
  anchorTop: number | null;
  height: number;
}

export interface MarginLayout {
  /** Each item's top relative to the margin. */
  tops: ReadonlyMap<string, number>;
  /** The items from the top: the placed ones by their text, then the rest. */
  order: readonly string[];
  /** The margin's extent: the bottom of its last item. */
  height: number;
}

export const MARGIN_GAP = 8;

export function layoutMargin(
  items: readonly MarginItemMeasure[],
  activeId: string | null,
  gap = MARGIN_GAP,
): MarginLayout {
  const placed = items
    .filter((item) => item.anchorTop !== null)
    .map((item, order) => ({ ...item, anchor: item.anchorTop!, order }))
    .sort((a, b) => a.anchor - b.anchor || a.order - b.order);
  const tops = placed.map((item) => item.anchor);
  const forward = (from: number, y: number) => {
    for (let i = from; i < placed.length; i++) {
      tops[i] = Math.max(placed[i].anchor, y);
      y = tops[i] + placed[i].height + gap;
    }
  };
  forward(0, 0);
  const active = placed.findIndex((item) => item.id === activeId);
  if (active >= 0 && tops[active] > placed[active].anchor) {
    tops[active] = placed[active].anchor;
    for (let i = active - 1; i >= 0; i--)
      tops[i] = Math.min(tops[i], tops[i + 1] - gap - placed[i].height);
    if (tops[0] < 0) {
      // Pushed above the top: the earlier items stack down from it, and the
      // active item gives way only as far as they reach.
      let y = 0;
      for (let i = 0; i < active; i++) {
        tops[i] = Math.max(tops[i], y);
        y = tops[i] + placed[i].height + gap;
      }
      tops[active] = Math.max(placed[active].anchor, y);
    }
    forward(active + 1, tops[active] + placed[active].height + gap);
  }
  const result = new Map<string, number>();
  let bottom = 0;
  placed.forEach((item, i) => {
    result.set(item.id, tops[i]);
    bottom = Math.max(bottom, tops[i] + item.height);
  });
  let y = placed.length ? bottom + gap : 0;
  for (const item of items) {
    if (item.anchorTop !== null) continue;
    result.set(item.id, y);
    y += item.height + gap;
    bottom = Math.max(bottom, y - gap);
  }
  return { tops: result, order: [...result.keys()], height: bottom };
}
