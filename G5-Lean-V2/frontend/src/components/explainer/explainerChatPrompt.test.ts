import { describe, expect, it } from "vitest";
import {
  EXPLAINER_QUESTION_INSTRUCTIONS,
  explainerSaveInstructions,
} from "./explainerChatPrompt";

describe("explainer question guidance", () => {
  it("teaches the CLI-created set and the host placeholder", () => {
    const text = explainerSaveInstructions("ws-1", "name");
    expect(text).toContain(EXPLAINER_QUESTION_INSTRUCTIONS);
    expect(EXPLAINER_QUESTION_INSTRUCTIONS).toContain("g5d question create");
    expect(EXPLAINER_QUESTION_INSTRUCTIONS).toContain(
      'data-g5-question="<set path>#<question id>"',
    );
    expect(EXPLAINER_QUESTION_INSTRUCTIONS).toContain("never edit a .g5q.json");
  });
});
