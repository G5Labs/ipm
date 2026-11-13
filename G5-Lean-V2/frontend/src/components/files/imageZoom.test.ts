import { describe, expect, it } from "vitest";
import {
  imageScale,
  MAX_ZOOM,
  MIN_ZOOM,
  steppedZoom,
  ZOOM_STEP,
} from "./imageZoom";

// `g5.documents.viewers.images`: fit never enlarges; zoom stays in bounds.
describe("image zoom", () => {
  const natural = { width: 2000, height: 1000 };

  it("fits a larger image into the panel and keeps a smaller one at 1", () => {
    expect(
      imageScale({ mode: "fit" }, natural, { width: 1000, height: 1000 }),
    ).toBe(0.5);
    expect(
      imageScale({ mode: "fit" }, natural, { width: 4000, height: 250 }),
    ).toBe(0.25);
    expect(
      imageScale(
        { mode: "fit" },
        { width: 10, height: 10 },
        { width: 500, height: 500 },
      ),
    ).toBe(1);
    expect(imageScale({ mode: "fit" }, natural, null)).toBe(1);
    expect(imageScale({ mode: "fit" }, natural, { width: 0, height: 0 })).toBe(
      1,
    );
  });

  it("honours actual size and chosen scales", () => {
    expect(
      imageScale({ mode: "actual" }, natural, { width: 10, height: 10 }),
    ).toBe(1);
    expect(imageScale({ mode: "scale", scale: 3 }, natural, null)).toBe(3);
  });

  it("steps by a fixed factor within the bounds", () => {
    expect(steppedZoom(1, 1)).toBeCloseTo(ZOOM_STEP);
    expect(steppedZoom(1, -1)).toBeCloseTo(1 / ZOOM_STEP);
    expect(steppedZoom(MAX_ZOOM, 1)).toBe(MAX_ZOOM);
    expect(steppedZoom(MIN_ZOOM, -1)).toBe(MIN_ZOOM);
  });
});
