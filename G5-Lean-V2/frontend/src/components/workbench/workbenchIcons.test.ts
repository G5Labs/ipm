import {
  ExternalLink,
  GitPullRequest,
  Hammer,
  List,
  MessageSquare,
  Stamp,
} from "lucide-react";
import { describe, expect, it } from "vitest";

import type { WorkbenchAction } from "../../lib/types";

// A served target this client does not know (an older client, a newer
// server): the type is narrower than what the route can carry.
const unknownTarget = (target: string) =>
  target as unknown as WorkbenchAction["target"];
import { workbenchActionIcon } from "./workbenchIcons";

// g5.workbench.header-styles: icons are client presentation keyed by the
// built-in id, then the open target, then the kind; any committed action id
// renders a glyph, including ids that name Object prototype members.
describe("workbenchActionIcon", () => {
  const chat = (id: string): WorkbenchAction => ({
    id,
    label: id,
    kind: "chat",
    prompt: "do it",
  });

  it("maps built-in ids and falls back by target, then by kind", () => {
    expect(workbenchActionIcon(chat("build-mock"))).toBe(Hammer);
    expect(
      workbenchActionIcon({
        id: "custom-approvals",
        label: "Sign-off",
        kind: "open",
        target: "approvals",
      }),
    ).toBe(Stamp);
    expect(
      workbenchActionIcon({
        id: "custom-ship",
        label: "Ship it",
        kind: "open",
        target: "ship",
      }),
    ).toBe(GitPullRequest);
    expect(workbenchActionIcon(chat("write-adr"))).toBe(MessageSquare);
    expect(
      workbenchActionIcon({
        id: "more",
        label: "More",
        kind: "menu",
        items: [],
      }),
    ).toBe(List);
    expect(
      workbenchActionIcon({
        id: "x",
        label: "x",
        kind: "open",
        target: unknownTarget("nope"),
      }),
    ).toBe(ExternalLink);
  });

  it("never resolves an id through the object prototype", () => {
    for (const id of [
      "constructor",
      "__proto__",
      "toString",
      "hasOwnProperty",
    ]) {
      expect(workbenchActionIcon(chat(id))).toBe(MessageSquare);
      expect(
        workbenchActionIcon({
          id: "open",
          label: id,
          kind: "open",
          target: unknownTarget(id),
        }),
      ).toBe(ExternalLink);
    }
  });
});
