// "Explain this section" seeds a chat whose explainer lands in the managed
// explainers root like every other one.
import { describe, expect, it } from "vitest";
import { explainDocSectionChatPrompt } from "./htmlDocAnswerBridge";

describe("explainDocSectionChatPrompt", () => {
  it("names the section and the managed explainer destination", () => {
    const prompt = explainDocSectionChatPrompt(
      "docs/ws-1/plan.html",
      "Rollout",
      "rollout",
      "ws-1",
    );
    expect(prompt).toContain(
      '"Rollout" (section id `rollout`) of the interactive project doc `docs/ws-1/plan.html`',
    );
    expect(prompt).toContain("under docs/ws-1/explainers/");
    expect(prompt).toContain("use project_docs/explainers/");
    expect(prompt).toContain("do not overwrite an earlier explainer");
  });
});
