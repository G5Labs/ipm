import { describe, expect, it } from "vitest";
import { placeAnchoredPopup } from "./popupPlacement";

const wide = { width: 1280, height: 800 };
const narrow = { width: 360, height: 640 };
const panel = { width: 600, height: 300 };

describe("anchored popup placement", () => {
  it("opens below the trigger when it fits", () => {
    expect(
      placeAnchoredPopup({ top: 100, bottom: 120, left: 200 }, panel, wide),
    ).toEqual({ top: 126, left: 200 });
  });
  it("flips above a trigger near the bottom edge", () => {
    expect(
      placeAnchoredPopup({ top: 700, bottom: 720, left: 200 }, panel, wide),
    ).toEqual({ top: 394, left: 200 });
  });
  it("clamps horizontally so a right-edge trigger stays on screen", () => {
    expect(
      placeAnchoredPopup({ top: 100, bottom: 120, left: 1200 }, panel, wide)
        .left,
    ).toBe(1280 - 8 - 600);
  });
  it("stays within a narrow viewport when neither side fits", () => {
    const tall = { width: 344, height: 600 };
    const placed = placeAnchoredPopup(
      { top: 300, bottom: 320, left: 300 },
      tall,
      narrow,
    );
    expect(placed.top).toBeGreaterThanOrEqual(8);
    expect(placed.top + tall.height).toBeLessThanOrEqual(640 - 8);
    expect(placed.left).toBe(8);
  });
  it("centres without an anchor", () => {
    expect(placeAnchoredPopup(null, panel, wide)).toEqual({
      top: 250,
      left: 340,
    });
  });
});
