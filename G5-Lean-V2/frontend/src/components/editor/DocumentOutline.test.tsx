// Documents g5.documents.question-index: the Outline lists `[Q<N>.]` records
// beside the headings, with their state, and scrolls to the record on press.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DocumentPreview } from "../files/DocumentPreview";

const DECISIONS = `# Decisions — Standup

Extracted from the meeting transcript.

[Q1. Remove the "do not rewrite branch history" instruction]

\`\`\`g5-question
schema: 1
id: history
status: open
prompt: Remove the instruction
options:
  - label: Yes
  - label: No
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
\`\`\`

[Q2. Adopt the main-protection invariant]

\`\`\`g5-question
schema: 1
id: invariant
status: answered
prompt: Adopt the invariant
options:
  - label: Yes
  - label: No
answer:
  selected:
    - Yes
  text: "yes"
  rationale: ""
  answered_by: user
  answered_at: "2026-10-22T12:00:00Z"
\`\`\`

[D1. Keep the merge mechanism open]

The agent chooses.
`;

function mount(content = DECISIONS) {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <DocumentPreview
        workspaceId=""
        repoId=""
        filePath="docs/ws/meetings/standup.decisions.md"
        content={content}
        canAnswer
        onRewrite={vi.fn(() => true)}
        onNavigate={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

describe("DocumentPreview outline question index", () => {
  it("lists each Q record with its state after the heading, and not the D record", () => {
    mount();
    const outline = document.getElementById("document-outline");
    expect(outline).not.toBeNull();
    expect(screen.getAllByTestId("document-outline-heading")).toHaveLength(1);
    const questions = screen.getAllByTestId("document-outline-question");
    expect(questions.map((q) => q.getAttribute("data-question-state"))).toEqual(
      ["open", "decided"],
    );
    expect(questions[0]).toHaveTextContent(
      'Q1 · Remove the "do not rewrite branch history" instruction',
    );
    expect(questions[1]).toHaveTextContent(
      "Q2 · Adopt the main-protection invariant",
    );
    expect(outline).not.toHaveTextContent("Keep the merge mechanism open");
  });

  it("scrolls to the record's frame on press", () => {
    mount();
    const frames = screen.getAllByTestId("semantic-record");
    const target = frames.find((frame) => frame.dataset.recordSpan === "Q2");
    expect(target).toBeDefined();
    const scrollIntoView = vi.fn();
    (target as HTMLElement).scrollIntoView = scrollIntoView;
    fireEvent.click(screen.getAllByTestId("document-outline-question")[1]);
    expect(scrollIntoView).toHaveBeenCalled();
  });

  it("keeps outline jumps inside its preview when another surface shares block indices", () => {
    const outside = document.createElement("div");
    outside.dataset.blockIndex = "0";
    outside.scrollIntoView = vi.fn();
    document.body.prepend(outside);
    try {
      const view = mount(
        "# Visible\n\n[I1. Ordinary]\n\nBody.\n\n## Filtered heading\n\n[D1. Other]\n\nOther body.\n",
      );
      const headings = screen.getAllByTestId("document-outline-heading");
      const visible = view.container.querySelector<HTMLElement>(
        '[data-block-index="0"]',
      )!;
      visible.scrollIntoView = vi.fn();
      fireEvent.click(headings[0]);
      expect(visible.scrollIntoView).toHaveBeenCalledOnce();
      expect(outside.scrollIntoView).not.toHaveBeenCalled();
      const filtered = screen.getByRole("heading", {
        name: "Filtered heading",
      });
      outside.dataset.blockIndex = filtered.dataset.blockIndex;
      fireEvent.click(
        screen.getByRole("button", { name: "Show human-authored blocks" }),
      );
      expect(
        screen.queryByRole("heading", { name: "Filtered heading" }),
      ).toBeNull();
      fireEvent.click(screen.getAllByTestId("document-outline-heading")[1]);
      expect(outside.scrollIntoView).not.toHaveBeenCalled();
    } finally {
      outside.remove();
    }
  });

  it("shows no outline for a document with neither headings nor Q records", () => {
    mount("Just a paragraph.\n");
    expect(document.getElementById("document-outline")).toBeNull();
  });
});
