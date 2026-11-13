// Host-owned Discuss composer template for doc question cards (center-panel
// D69): names the question id, sanitized title, doc path, option labels, and
// the current recommendation; missing fields degrade the template. Also
// covers the seed-attached reference merge used by the seeded session's
// starting send.
import { describe, expect, it } from "vitest";
import {
  buildQuestionDiscussPrompt,
  mergeSeededChatReference,
} from "./WorkspacePage";
import { parseQuestionPayload } from "../lib/questionBlock";
import type { ChatProjectReference } from "../lib/types";

function question(payload: string) {
  const parsed = parseQuestionPayload(payload, { fallbackId: "q1" });
  if (!parsed.ok) throw new Error("test payload must parse");
  return parsed.question;
}

const FULL_PAYLOAD = [
  "schema: 1",
  "id: pick-color",
  "prompt: Which color should the button use?",
  "options:",
  "  - label: Red",
  "    description: red token",
  "  - label: Blue",
  "    description: info token",
  "    recommended: true",
].join("\n");

describe("buildQuestionDiscussPrompt", () => {
  it("renders the full template with title, prompt, options, and recommendation", () => {
    const prompt = buildQuestionDiscussPrompt(
      question(FULL_PAYLOAD),
      "design.md",
      "Pick a color",
    );
    expect(prompt).toBe(
      'Let\'s discuss question pick-color ("Pick a color") in ' +
        "Project Docs/design.md: Which color should the button use? " +
        "The proposed options are Red, Blue; the current recommendation is " +
        "Blue. Help me reason about the tradeoffs before I answer — do not " +
        "edit the doc until I decide.",
    );
  });

  it("sanitizes backticks, control characters, and whitespace in title and prompt", () => {
    const payload = [
      "schema: 1",
      "id: q-dirty",
      'prompt: "line one\\nline\\ttwo `rm -rf`"',
      "options: []",
    ].join("\n");
    const prompt = buildQuestionDiscussPrompt(
      question(payload),
      "design.md",
      "Title `with` \x07 bell\nand newline",
    );
    expect(prompt).toContain('("Title with bell and newline")');
    expect(prompt).toContain("line one line two rm -rf");
    expect(prompt).not.toContain("`");
    expect(prompt).not.toContain("\x07");
    expect(prompt).not.toContain("\n");
  });

  it("truncates the sanitized title to the 200-character cap", () => {
    const longTitle = "t".repeat(250);
    const prompt = buildQuestionDiscussPrompt(
      question(FULL_PAYLOAD),
      "design.md",
      longTitle,
    );
    expect(prompt).toContain(`("${"t".repeat(200)}")`);
    expect(prompt).not.toContain("t".repeat(201));
  });

  it("drops the options sentence for a zero-option question and the title parenthetical when absent", () => {
    const payload = [
      "schema: 1",
      "id: free-form",
      "prompt: Anything else?",
      "options: []",
    ].join("\n");
    const prompt = buildQuestionDiscussPrompt(question(payload), "notes.md");
    expect(prompt).toBe(
      "Let's discuss question free-form in Project Docs/notes.md: " +
        "Anything else? Help me reason about the tradeoffs before I answer " +
        "— do not edit the doc until I decide.",
    );
    expect(prompt).not.toContain("The proposed options are");
    expect(prompt).not.toContain('("');
  });

  it("keeps the options sentence but drops the recommendation clause when none exists", () => {
    const payload = [
      "schema: 1",
      "id: pick-color",
      "prompt: Which color?",
      "options:",
      "  - label: Red",
      "    description: red token",
      "  - label: Blue",
      "    description: info token",
    ].join("\n");
    const prompt = buildQuestionDiscussPrompt(question(payload), "design.md");
    expect(prompt).toContain("The proposed options are Red, Blue.");
    expect(prompt).not.toContain("current recommendation");
  });
});

describe("mergeSeededChatReference", () => {
  const seeded: ChatProjectReference = {
    id: "project_doc:doc-1",
    kind: "project_doc",
    label: "design.md",
    docId: "doc-1",
  };

  it("returns the original references when nothing was seeded", () => {
    expect(mergeSeededChatReference(undefined, null)).toBeUndefined();
    const picked = [seeded];
    expect(mergeSeededChatReference(picked, null)).toBe(picked);
  });

  it("appends the seeded reference to user-picked references", () => {
    const other: ChatProjectReference = {
      id: "source_file:workspace_service",
      kind: "source_file",
      label: "workspace_service",
      path: "src/workspace_service.rs",
    };
    expect(mergeSeededChatReference([other], seeded)).toEqual([other, seeded]);
    expect(mergeSeededChatReference(undefined, seeded)).toEqual([seeded]);
  });

  it("deduplicates when the user already attached the same doc", () => {
    expect(mergeSeededChatReference([seeded], seeded)).toEqual([seeded]);
  });
});
