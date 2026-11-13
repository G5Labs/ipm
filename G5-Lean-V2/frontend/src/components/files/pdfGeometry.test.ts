import { describe, expect, it } from "vitest";
import {
  canvasOutputScale,
  CSS_UNITS,
  describePdfError,
  fitWidthScale,
  MAX_CANVAS_PIXELS,
  nextZoomPreset,
  rotatedSize,
  ZOOM_PRESETS,
} from "./pdfGeometry";

// `g5.documents.viewers.pdf`: canvases stay under the pixel cap; zoom
// presets and fit width are the toolbar's arithmetic.
describe("pdf geometry", () => {
  it("caps the canvas at the pixel bound and the device ratio at 2", () => {
    expect(canvasOutputScale(612, 792, 1)).toBe(1);
    expect(canvasOutputScale(612, 792, 2)).toBe(2);
    expect(canvasOutputScale(612, 792, 3)).toBe(2);
    expect(canvasOutputScale(612, 792, 0)).toBe(1);
    // A page at a zoom whose canvas would exceed the cap is scaled down.
    const scale = canvasOutputScale(6000, 6000, 2);
    expect(scale).toBeLessThan(1);
    expect(6000 * 6000 * scale * scale).toBeLessThanOrEqual(
      MAX_CANVAS_PIXELS + 1,
    );
    expect(canvasOutputScale(0, 0, 2)).toBe(2);
  });

  it("fits a page width into the available width", () => {
    expect(fitWidthScale(612, 612)).toBe(1);
    expect(fitWidthScale(1224, 612)).toBe(2);
    expect(fitWidthScale(0, 612)).toBe(CSS_UNITS);
    expect(fitWidthScale(500, 0)).toBe(CSS_UNITS);
  });

  it("steps through the presets and clamps at their ends", () => {
    expect(nextZoomPreset(1, 1)).toBe(1.25);
    expect(nextZoomPreset(1, -1)).toBe(0.75);
    expect(nextZoomPreset(1.1, 1)).toBe(1.25);
    expect(nextZoomPreset(1.1, -1)).toBe(1);
    expect(nextZoomPreset(ZOOM_PRESETS[ZOOM_PRESETS.length - 1], 1)).toBe(4);
    expect(nextZoomPreset(ZOOM_PRESETS[0], -1)).toBe(0.5);
  });

  it("swaps page sides for quarter turns", () => {
    expect(rotatedSize({ width: 2, height: 3 }, 0)).toEqual({
      width: 2,
      height: 3,
    });
    expect(rotatedSize({ width: 2, height: 3 }, 90)).toEqual({
      width: 3,
      height: 2,
    });
    expect(rotatedSize({ width: 2, height: 3 }, 180)).toEqual({
      width: 2,
      height: 3,
    });
    expect(rotatedSize({ width: 2, height: 3 }, 270)).toEqual({
      width: 3,
      height: 2,
    });
  });

  it("names invalid and protected documents and quotes other failures", () => {
    expect(
      describePdfError({ name: "InvalidPDFException", message: "x" }),
    ).toBe("This file is not a valid PDF.");
    expect(describePdfError({ name: "PasswordException" })).toBe(
      "This PDF is password protected.",
    );
    expect(describePdfError(new Error("worker died"))).toBe(
      "This PDF could not be opened: worker died",
    );
    expect(describePdfError(undefined)).toBe("This PDF could not be opened.");
  });
});
