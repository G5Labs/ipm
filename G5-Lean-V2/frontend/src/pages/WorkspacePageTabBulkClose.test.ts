import { describe, expect, it } from "vitest";
import { tabsToKeepForBulkClose } from "./WorkspacePage";
import type { TabItem } from "../lib/types";

// Helper to build a simple doc tab whose id is `doc:<id>`.
const docTab = (id: string): TabItem => ({
  type: "doc",
  docId: id,
  name: id,
});

const ids = (tabs: TabItem[]): string[] =>
  tabs.map((t) => (t.type === "doc" ? t.docId : ""));

describe("tabsToKeepForBulkClose", () => {
  const tabs = [docTab("a"), docTab("b"), docTab("c"), docTab("d")];

  describe('scope "others"', () => {
    it("keeps only the anchor tab", () => {
      expect(ids(tabsToKeepForBulkClose(tabs, "doc:b", "others"))).toEqual([
        "b",
      ]);
    });

    it("keeps the anchor even when it is the first tab", () => {
      expect(ids(tabsToKeepForBulkClose(tabs, "doc:a", "others"))).toEqual([
        "a",
      ]);
    });

    it("keeps the anchor even when it is the last tab", () => {
      expect(ids(tabsToKeepForBulkClose(tabs, "doc:d", "others"))).toEqual([
        "d",
      ]);
    });
  });

  describe('scope "right"', () => {
    it("keeps the anchor and every tab to its left", () => {
      expect(ids(tabsToKeepForBulkClose(tabs, "doc:b", "right"))).toEqual([
        "a",
        "b",
      ]);
    });

    it("is a no-op when the anchor is the right-most tab", () => {
      expect(ids(tabsToKeepForBulkClose(tabs, "doc:d", "right"))).toEqual([
        "a",
        "b",
        "c",
        "d",
      ]);
    });

    it("keeps only the first tab when anchor is the first tab", () => {
      expect(ids(tabsToKeepForBulkClose(tabs, "doc:a", "right"))).toEqual([
        "a",
      ]);
    });
  });

  it("returns the list unchanged when the anchor id is not present", () => {
    expect(tabsToKeepForBulkClose(tabs, "doc:missing", "others")).toBe(tabs);
    expect(tabsToKeepForBulkClose(tabs, "doc:missing", "right")).toBe(tabs);
  });

  it("preserves left-to-right ordering of the surviving tabs", () => {
    const reordered = [docTab("x"), docTab("y"), docTab("z")];
    expect(ids(tabsToKeepForBulkClose(reordered, "doc:y", "right"))).toEqual([
      "x",
      "y",
    ]);
  });
});
