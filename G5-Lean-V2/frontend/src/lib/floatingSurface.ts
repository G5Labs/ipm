// =============================================================================
// G5 Client — floating surface ownership
// =============================================================================
// A popover or dialog portaled to the body (the ship-approvals roster used by
// the status band and the Guided section; every `Modal`) is not a DOM
// descendant of the surface that opened it. Surfaces with outside-click or
// hover-leave rules (the left sidebar's rail overlay, left-sidebar D38) read
// this marker to stay open while the pointer, a click or focus is in such a
// floating surface, so the controls inside it keep working.
// =============================================================================

export const FLOATING_SURFACE_ATTR = "data-floating-surface";

/** Whether an event target (or a focus/hover related target) is inside a floating surface. */
export function withinFloatingSurface(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest(`[${FLOATING_SURFACE_ATTR}]`) !== null
  );
}

/**
 * Whether any floating surface is open: a blur with no related target (a
 * click on non-focusable text inside one) must not count as leaving the
 * opening surface while one is.
 */
export function floatingSurfaceOpen(): boolean {
  return document.querySelector(`[${FLOATING_SURFACE_ATTR}]`) !== null;
}
