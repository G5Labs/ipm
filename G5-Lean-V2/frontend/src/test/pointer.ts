import { fireEvent } from "@testing-library/react";

/**
 * jsdom has no PointerEvent: a MouseEvent of the pointer type carries the
 * coordinates React's pointer handlers read, plus the pointer id they match.
 */
export function firePointer(
  el: Element,
  type: "pointerdown" | "pointermove" | "pointerup" | "pointercancel",
  clientX: number,
  clientY: number,
  pointerId = 1,
): void {
  const event = new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX,
    clientY,
  });
  Object.defineProperty(event, "pointerId", { value: pointerId });
  fireEvent(el, event);
}
