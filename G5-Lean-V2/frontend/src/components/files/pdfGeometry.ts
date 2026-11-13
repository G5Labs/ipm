// =============================================================================
// G5 Files — pdfGeometry
// =============================================================================
// The PDF viewer's bounds, zoom arithmetic and error wording
// (`g5.documents.viewers.pdf`).
// =============================================================================

/** The most pixels one page canvas may hold (about 64 MiB of RGBA). */
export const MAX_CANVAS_PIXELS = 16 * 1024 * 1024;
/** PDF points to CSS pixels: 100% shows a page at 96 dpi, as browsers do. */
export const CSS_UNITS = 96 / 72;
export const ZOOM_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

export interface Size {
  width: number;
  height: number;
}

export type PdfZoom = { mode: "fit-width" } | { mode: "fixed"; zoom: number };

/**
 * The device pixels per CSS pixel a page canvas uses: the device ratio,
 * at most 2, lowered so the canvas stays within `MAX_CANVAS_PIXELS`.
 */
export function canvasOutputScale(
  width: number,
  height: number,
  devicePixelRatio: number,
): number {
  const ratio = Math.min(Math.max(devicePixelRatio || 1, 1), 2);
  const pixels = width * height;
  if (!(pixels > 0)) return ratio;
  return Math.min(ratio, Math.sqrt(MAX_CANVAS_PIXELS / pixels));
}

/** The PDF.js scale that fits a page of `pageWidth` points into `availableWidth`. */
export function fitWidthScale(
  availableWidth: number,
  pageWidth: number,
): number {
  if (!(availableWidth > 0) || !(pageWidth > 0)) return CSS_UNITS;
  return availableWidth / pageWidth;
}

/** The next zoom preset above or below `zoom` (1 = 100%), clamped. */
export function nextZoomPreset(zoom: number, direction: 1 | -1): number {
  if (direction > 0) {
    return (
      ZOOM_PRESETS.find((preset) => preset > zoom + 1e-6) ??
      ZOOM_PRESETS[ZOOM_PRESETS.length - 1]
    );
  }
  const lower = ZOOM_PRESETS.filter((preset) => preset < zoom - 1e-6);
  return lower.length ? lower[lower.length - 1] : ZOOM_PRESETS[0];
}

export function rotatedSize(size: Size, rotation: number): Size {
  return rotation % 180 === 0
    ? size
    : { width: size.height, height: size.width };
}

/** What to tell the person when a document cannot be opened. */
export function describePdfError(error: unknown): string {
  const name = (error as { name?: unknown } | null)?.name;
  if (name === "InvalidPDFException") return "This file is not a valid PDF.";
  if (name === "PasswordException") return "This PDF is password protected.";
  const message =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : "";
  return message
    ? `This PDF could not be opened: ${message}`
    : "This PDF could not be opened.";
}
