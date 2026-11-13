import { describe, expect, it } from "vitest";
import { layoutMargin } from "./documentMarginLayout";

const item = (id: string, anchorTop: number | null, height = 40) => ({
  id,
  anchorTop,
  height,
});

describe("layoutMargin", () => {
  it("sets each item level with its text when nothing is in the way", () => {
    const { tops, height } = layoutMargin(
      [item("a", 10), item("b", 100), item("c", 300)],
      null,
      8,
    );
    expect([...tops]).toEqual([
      ["a", 10],
      ["b", 100],
      ["c", 300],
    ]);
    expect(height).toBe(340);
  });

  it("orders items by their anchors and pushes later ones down to make room", () => {
    const { tops } = layoutMargin(
      [item("late", 200), item("first", 0, 100), item("second", 30)],
      null,
      8,
    );
    expect(tops.get("first")).toBe(0);
    expect(tops.get("second")).toBe(108);
    expect(tops.get("late")).toBe(200);
  });

  it("keeps the order given for items on the same text", () => {
    const { tops } = layoutMargin(
      [item("suggestion", 50), item("thread", 50)],
      null,
      8,
    );
    expect(tops.get("suggestion")).toBe(50);
    expect(tops.get("thread")).toBe(98);
  });

  it("sets the active item level with its text and pushes the ones before it up", () => {
    const items = [item("a", 0, 30), item("b", 100, 30), item("c", 110, 30)];
    expect(layoutMargin(items, null, 8).tops.get("c")).toBe(138);
    const { tops } = layoutMargin(items, "c", 8);
    expect(tops.get("c")).toBe(110);
    expect(tops.get("b")).toBe(72);
    expect(tops.get("a")).toBe(0);
  });

  it("pushes the items after the active one down", () => {
    const { tops } = layoutMargin(
      [item("a", 0, 30), item("b", 20, 30), item("c", 30, 30)],
      "a",
      8,
    );
    expect(tops.get("a")).toBe(0);
    expect(tops.get("b")).toBe(38);
    expect(tops.get("c")).toBe(76);
  });

  it("never pushes items above the margin's top: the active item gives way instead", () => {
    const items = [item("a", 0, 30), item("b", 50, 30), item("c", 60, 30)];
    expect(layoutMargin(items, null, 8).tops.get("c")).toBe(88);
    const { tops } = layoutMargin(items, "c", 8);
    expect(tops.get("a")).toBe(0);
    expect(tops.get("b")).toBe(38);
    expect(tops.get("c")).toBe(76);
  });

  it("lists items without a place after the placed ones, in the order given", () => {
    const { tops, height } = layoutMargin(
      [item("orphan", null, 30), item("b", 100), item("unanchored", null, 20)],
      null,
      8,
    );
    expect(tops.get("b")).toBe(100);
    expect(tops.get("orphan")).toBe(148);
    expect(tops.get("unanchored")).toBe(186);
    expect(height).toBe(206);
  });

  it("stacks items from the top when none has a place", () => {
    const { tops, height } = layoutMargin(
      [item("x", null, 30), item("y", null, 30)],
      null,
      8,
    );
    expect(tops.get("x")).toBe(0);
    expect(tops.get("y")).toBe(38);
    expect(height).toBe(68);
  });
});
