import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

export const CHANGE_GRAPH_ZOOM_CONTROL_STEP = 0.05;

const MODIFIER_WHEEL_ZOOM_STEP = 0.08;
const MIN_SCALE = 0.45;
const MAX_SCALE = 1.6;
const PAN_THRESHOLD_PX = 3;

interface UseChangeGraphViewportOptions {
  worldWidth: number;
  worldHeight: number;
  onBackgroundClick?: () => void;
}

export function useChangeGraphViewport({
  worldWidth,
  worldHeight,
  onBackgroundClick,
}: UseChangeGraphViewportOptions) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const dragRef = useRef<{
    pointerId: number;
    x: number;
    y: number;
    left: number;
    top: number;
    moved: boolean;
  } | null>(null);

  const changeScale = useCallback((delta: number) => {
    setScale((current) =>
      Math.min(MAX_SCALE, Math.max(MIN_SCALE, current + delta)),
    );
  }, []);

  const fitGraph = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const next = Math.min(
      1,
      Math.max(
        MIN_SCALE,
        Math.min(
          viewport.clientWidth / worldWidth,
          viewport.clientHeight / worldHeight,
        ),
      ),
    );
    setScale(next);
    viewport.scrollTo({ left: 0, top: 0, behavior: "smooth" });
  }, [worldHeight, worldWidth]);

  const onPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const viewport = viewportRef.current;
      if (event.button !== 0 || !viewport) return;
      const target = event.target as HTMLElement;
      if (target.closest("button, a, input, select, textarea, summary")) return;
      event.currentTarget.setPointerCapture(event.pointerId);
      dragRef.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        left: viewport.scrollLeft,
        top: viewport.scrollTop,
        moved: false,
      };
    },
    [],
  );

  const onPointerMove = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current;
      const viewport = viewportRef.current;
      if (!drag || !viewport || drag.pointerId !== event.pointerId) return;
      const dx = event.clientX - drag.x;
      const dy = event.clientY - drag.y;
      if (!drag.moved && Math.hypot(dx, dy) < PAN_THRESHOLD_PX) return;
      drag.moved = true;
      viewport.scrollLeft = drag.left - dx;
      viewport.scrollTop = drag.top - dy;
    },
    [],
  );

  const releasePointer = useCallback(
    (element: HTMLDivElement, pointerId: number) => {
      if (element.hasPointerCapture?.(pointerId)) {
        element.releasePointerCapture(pointerId);
      }
    },
    [],
  );

  const onPointerUp = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      const moved =
        drag.moved ||
        Math.hypot(event.clientX - drag.x, event.clientY - drag.y) >=
          PAN_THRESHOLD_PX;
      dragRef.current = null;
      releasePointer(event.currentTarget, event.pointerId);
      if (!moved) onBackgroundClick?.();
    },
    [onBackgroundClick, releasePointer],
  );

  const onPointerCancel = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      dragRef.current = null;
      releasePointer(event.currentTarget, event.pointerId);
    },
    [releasePointer],
  );

  const onWheel = useCallback(
    (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      changeScale(
        event.deltaY < 0 ? MODIFIER_WHEEL_ZOOM_STEP : -MODIFIER_WHEEL_ZOOM_STEP,
      );
    },
    [changeScale],
  );

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [onWheel]);

  return {
    viewportRef,
    scale,
    changeScale,
    fitGraph,
    viewportPointerHandlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel,
    },
  };
}
