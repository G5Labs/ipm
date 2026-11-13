import { describe, expect, it } from "vitest";
import type { TabItem } from "../lib/types";
import { knownTabsOnly } from "./WorkspacePage";

describe("retired Progress tab", () => {
  it("drops an old dashboard while retaining supported sibling tabs", () => {
    const plan = {
      type: "workspace_plan",
      name: "Progress",
    } as unknown as TabItem;
    const file: TabItem = {
      type: "file",
      name: "request.md",
      filePath: "docs/ws/request.md",
    };
    expect(knownTabsOnly([plan, file])).toEqual([file]);
    expect(knownTabsOnly([plan])).toEqual([]);
  });
});
