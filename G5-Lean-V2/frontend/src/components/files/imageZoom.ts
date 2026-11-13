// =============================================================================
// G5 Files — imageZoom
// =============================================================================
// The image viewer's bounds and zoom arithmetic (`g5.documents.viewers.images`).
// =============================================================================

import type { ImageDimensions } from "./imageDimensions";

/** 64 megapixels: about 256 MiB decoded, whatever the compressed size. */
export const MAX_IMAGE_PIXELS = 64 * 1024 * 1024;
export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 8;
export const ZOOM_STEP = 1.25;

export type ImageZoom =
  | { mode: "fit" }
  | { mode: "actual" }
  | { mode: "scale"; scale: number };

export interface PanelSize {
  width: number;
  height: number;
}

/**
 * The scale an image shows at: fit shrinks a larger image into the panel
 * and never enlarges a smaller one; actual is 1; a chosen scale is itself.
 */
export function imageScale(
  zoom: ImageZoom,
  natural: ImageDimensions | null,
  panel: PanelSize | null,
): number {
  switch (zoom.mode) {
    case "actual":
      return 1;
    case "scale":
      return zoom.scale;
    case "fit":
      if (!natural || !panel || panel.width <= 0 || panel.height <= 0) return 1;
      return Math.min(
        1,
        panel.width / natural.width,
        panel.height / natural.height,
      );
  }
}

/** The next zoom step in or out from `scale`, within the bounds. */
export function steppedZoom(scale: number, direction: 1 | -1): number {
  const next = direction > 0 ? scale * ZOOM_STEP : scale / ZOOM_STEP;
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, next));
}
