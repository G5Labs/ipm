// =============================================================================
// DocumentMargin — cards level with their text beside the rendered document
// =============================================================================
// The margin to the right of a rendered document holds its suggestion cards,
// comment threads and the new-thread draft, each level with the text it is
// about, later cards pushed down to make room, and the active card (last
// expanded, selected or reached from its inline mark) set level with its text
// again. The margin owns measurement only: it reads where each item's text is
// relative to the row holding the document and the margin, watches sizes,
// and places the items with `layoutMargin`. The items are rendered keyed by
// id, so a card keeps its component, and with it what is typed in it, a send
// in flight and its error, when it moves (`g5.documents.comments.identity`,
// `g5.documents.review-placement`, `g5.documents.comments.failure`).
// =============================================================================

import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { layoutMargin, type MarginLayout } from "./documentMarginLayout";

export interface DocumentMarginItem {
  id: string;
  /** The element the item is level with; null while it has no place. */
  anchor: () => HTMLElement | null;
  node: React.ReactNode;
}

export interface DocumentMarginProps {
  items: readonly DocumentMarginItem[];
  /** The item set level with its text, pushing its neighbours away. */
  activeId: string | null;
  /** The row holding the document: positions are relative to it. */
  rowRef: React.RefObject<HTMLElement | null>;
  /**
   * The document's content: a change in it (a preview switching to another
   * alternative, say) can move an anchor without changing any size.
   */
  contentRef?: React.RefObject<HTMLElement | null>;
  /**
   * The row is too stacked for a margin beside the document: the items are
   * listed under it instead, in the same order (`useRowStacked`).
   */
  stacked?: boolean;
  testId?: string;
  /** Shown above the items: a notice, or the empty state when there are none. */
  children?: React.ReactNode;
}

const EMPTY: MarginLayout = { tops: new Map(), order: [], height: 0 };

/** Below this row width the margin stacks under the document instead. */
export const MARGIN_MIN_ROW_WIDTH = 768;

/** Whether the row is too stacked for a margin beside it; follows its size. */
export function useRowStacked(
  rowRef: React.RefObject<HTMLElement | null>,
): boolean {
  const [stacked, setStacked] = useState(false);
  useEffect(() => {
    const row = rowRef.current;
    if (!row) return;
    const check = () => {
      const width = row.clientWidth;
      setStacked(width > 0 && width < MARGIN_MIN_ROW_WIDTH);
    };
    check();
    window.addEventListener("resize", check);
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(check);
    observer?.observe(row);
    return () => {
      window.removeEventListener("resize", check);
      observer?.disconnect();
    };
  }, [rowRef]);
  return stacked;
}

export const DocumentMargin: React.FC<DocumentMarginProps> = ({
  items,
  activeId,
  rowRef,
  contentRef,
  stacked = false,
  testId = "document-margin",
  children,
}) => {
  const elements = useRef(new Map<string, HTMLDivElement>());
  const [layout, setLayout] = useState<MarginLayout>(EMPTY);
  // What the observers measure: written once the render commits, so a
  // discarded render never leaks into a later measurement.
  const latest = useRef({ items, activeId });
  useLayoutEffect(() => {
    latest.current = { items, activeId };
  });
  const measure = useCallback(() => {
    const row = rowRef.current;
    if (!row) return;
    const rowTop = row.getBoundingClientRect().top;
    const { items, activeId } = latest.current;
    const next = layoutMargin(
      items.map((item) => {
        const anchor = item.anchor();
        return {
          id: item.id,
          anchorTop: anchor
            ? Math.max(0, anchor.getBoundingClientRect().top - rowTop)
            : null,
          height: elements.current.get(item.id)?.offsetHeight ?? 0,
        };
      }),
      activeId,
    );
    setLayout((current) =>
      current.height === next.height &&
      current.order.length === next.order.length &&
      next.order.every(
        (id, i) =>
          current.order[i] === id && current.tops.get(id) === next.tops.get(id),
      )
        ? current
        : next,
    );
  }, [rowRef]);
  // The items (their places may have changed with the host's render) or the
  // active one changed: place them before the frame is shown. The observer
  // below measures again once the document has laid out.
  useLayoutEffect(measure, [measure, items, activeId]);
  const itemKey = items.map((item) => item.id).join("\n");
  // The document or a card changed size, or the document's content changed
  // under the same size: place them again.
  useEffect(() => {
    const row = rowRef.current;
    if (!row) return;
    const resize =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => measure());
    resize?.observe(row);
    const content = contentRef?.current;
    if (content) resize?.observe(content);
    for (const element of elements.current.values()) resize?.observe(element);
    // Mutations come in bursts: one measurement per frame.
    let pending = false;
    const later =
      typeof requestAnimationFrame === "function"
        ? requestAnimationFrame
        : (callback: () => void) => window.setTimeout(callback, 0);
    const mutation =
      content && typeof MutationObserver !== "undefined"
        ? new MutationObserver(() => {
            if (pending) return;
            pending = true;
            later(() => {
              pending = false;
              measure();
            });
          })
        : null;
    mutation?.observe(content!, {
      childList: true,
      characterData: true,
      subtree: true,
      attributes: true,
    });
    window.addEventListener("resize", measure);
    return () => {
      resize?.disconnect();
      mutation?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [measure, rowRef, contentRef, itemKey]);
  const adopt = (id: string) => (element: HTMLDivElement | null) => {
    if (element) elements.current.set(id, element);
    else elements.current.delete(id);
  };
  const byId = new Map(items.map((item) => [item.id, item]));
  const ordered = [
    ...layout.order.flatMap((id) => byId.get(id) ?? []),
    ...items.filter((item) => !layout.tops.has(item.id)),
  ];
  return (
    <aside
      data-testid={testId}
      data-layout={stacked ? "stacked" : "aligned"}
      className={
        stacked
          ? "w-full space-y-2 border-t border-[var(--g5-stroke)] px-2 py-2"
          : "relative w-[320px] shrink-0 border-l border-[var(--g5-stroke)] py-2"
      }
      style={stacked ? undefined : { minHeight: layout.height + 16 }}
    >
      {children}
      {ordered.map((item) => (
        <div
          key={item.id}
          ref={adopt(item.id)}
          data-margin-item={item.id}
          data-active={item.id === activeId ? "true" : undefined}
          className={
            stacked
              ? undefined
              : "absolute left-2 right-2 transition-[top] duration-150"
          }
          style={stacked ? undefined : { top: layout.tops.get(item.id) ?? 0 }}
        >
          {item.node}
        </div>
      ))}
    </aside>
  );
};
