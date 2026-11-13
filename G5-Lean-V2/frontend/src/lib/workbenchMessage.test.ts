import { describe, expect, it } from "vitest";

import {
  workbenchActionContext,
  workbenchActionMessage,
} from "./workbenchMessage";

// Mirrors `workbench_action_message` in
// backend/g5-api-core/src/services/agents/workbench_config.rs: the same text an
// append sends is what the server builds for a start (minus the persona).
describe("workbenchActionMessage", () => {
  it("serializes the prompt, the phase context and the typed text", () => {
    expect(
      workbenchActionMessage(
        { prompt: "Do the thing." },
        "se",
        { kind: "phase" },
        "  and hurry  ",
      ),
    ).toBe(
      "Do the thing.\n\nContext: the workspace is in the System Engineering phase.\n\nand hurry",
    );
  });

  it("names the current document or asks for one", () => {
    expect(
      workbenchActionMessage(
        { prompt: "Import." },
        "be",
        { kind: "current_doc", docPath: "docs/ws/backlog.csv" },
        "",
      ),
    ).toBe(
      "Import.\n\nContext: the current document is `docs/ws/backlog.csv`.",
    );
    expect(
      workbenchActionMessage(
        { prompt: "Import." },
        "be",
        { kind: "current_doc" },
        "",
      ),
    ).toBe("Import.\n\nContext: no document is open; ask which one to use.");
    expect(workbenchActionMessage({ prompt: "P" }, "be", undefined, "")).toBe(
      "P",
    );
    expect(
      workbenchActionMessage({ prompt: "P" }, "be", { kind: "none" }, ""),
    ).toBe("P");
  });

  it("resolves the action's declared context against the page", () => {
    expect(
      workbenchActionContext({ context: "current_doc" }, "docs/a.md"),
    ).toEqual({
      kind: "current_doc",
      docPath: "docs/a.md",
    });
    expect(workbenchActionContext({ context: "current_doc" }, null)).toEqual({
      kind: "current_doc",
      docPath: undefined,
    });
    expect(workbenchActionContext({ context: "phase" }, null)).toEqual({
      kind: "phase",
    });
    expect(workbenchActionContext({}, null)).toBeUndefined();
  });
});
