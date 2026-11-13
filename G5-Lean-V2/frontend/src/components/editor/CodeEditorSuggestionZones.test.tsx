/**
 * CodeEditor loading fallback: review suggestions are shown inline under
 * the line they target, the same cards Monaco shows in its view zones, and
 * the line gutter makes room for them.
 */
import React from "react";
import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { renderApp } from "../../test/render";
import type { ReviewSuggestion } from "../../lib/types";
import { CodeEditor } from "./CodeEditor";

// Keep the native editor loading while checking fallback row geometry.
vi.mock("./loadMonaco", () => ({
  loadMonacoReact: () => new Promise(() => {}),
}));

const SOURCE_ID = "ws-source-1";

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "sug-1",
    publicationId: "pub-1",
    findingId: null,
    repoId: "repo-api",
    path: "src/main.rs",
    kind: "hunk",
    beforeLines: "return 1;\n",
    afterLines: "return 0;\n",
    anchorStart: 3,
    contentHash: "abc",
    anchorContext: null,
    authorKind: "human",
    authorLabel: "alex",
    note: "",
    commentId: "c-1",
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-09-23T14:31:00.000Z",
    ...overrides,
  };
}

describe("CodeEditor suggestion zones (loading fallback)", () => {
  it("retains the existing new-file proposal presentation for FILES", async () => {
    const before = "Original document.\n";
    const after = "Updated document.\n\nAdded paragraph.\n";
    renderApp(
      <CodeEditor
        filePath="docs/spec.md"
        content={before}
        language="markdown"
        editable={false}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
        suggestionOverlay={{
          workspaceId: SOURCE_ID,
          viewerOwnsWorkspace: true,
          viewerUserId: "owner",
          ownerVmRunning: false,
          sourceWritable: true,
          suggestions: [
            suggestion({
              id: "doc-update",
              kind: "new_file",
              path: "docs/reviewer/spec.md",
              beforeLines: null,
              afterLines: after,
              anchorStart: null,
            }),
          ],
        }}
      />,
    );
    const card = await screen.findByTestId("review-suggestion-doc-update");
    expect([...card.querySelectorAll("pre")].map((p) => p.textContent)).toEqual(
      [after],
    );
    expect(
      screen.queryByTestId("spec-suggestion-preview-doc-update"),
    ).not.toBeInTheDocument();
  });

  it("renders each suggestion's card right under its line, with a gutter spacer", async () => {
    renderApp(
      <CodeEditor
        filePath="src/main.rs"
        content={"fn main() {\n    let x = 1;\n    return 1;\n}\n"}
        language="rust"
        editable={false}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
        suggestionOverlay={{
          workspaceId: SOURCE_ID,
          suggestions: [
            suggestion({ id: "sug-1", anchorStart: 3 }),
            suggestion({
              id: "sug-top",
              kind: "new_file",
              path: "NEW.md",
              beforeLines: null,
              afterLines: "# New\n",
              anchorStart: null,
            }),
          ],
          viewerOwnsWorkspace: true,
          viewerUserId: "user-owner",
          ownerVmRunning: false,
          sourceWritable: true,
        }}
      />,
    );
    const zone = await screen.findByTestId("suggestion-zone-after-3");
    // The zone follows the line it targets.
    expect(zone.previousElementSibling).toHaveAttribute(
      "data-testid",
      "code-line-3",
    );
    expect(zone.previousElementSibling).toHaveTextContent("return 1;");
    expect(
      within(zone).getByTestId("review-suggestion-sug-1"),
    ).toHaveTextContent("−return 1;");
    // The owner's actions are there; Accept waits for the running VM.
    expect(
      within(zone).getByTestId("review-suggestion-accept-sug-1"),
    ).toBeDisabled();
    expect(
      within(zone).getByTestId("review-suggestion-edit-toggle-sug-1"),
    ).toBeEnabled();
    // A suggestion without an anchor sits above the first line.
    const top = screen.getByTestId("suggestion-zone-after-0");
    expect(top.nextElementSibling).toHaveAttribute(
      "data-testid",
      "code-line-1",
    );
    // The line gutter reserves the zones' heights (0 in jsdom, but present).
    expect(screen.getByTestId("suggestion-zone-gutter-3")).toBeInTheDocument();
    expect(screen.getByTestId("suggestion-zone-gutter-0")).toBeInTheDocument();
    // No side rail any more.
    await waitFor(() =>
      expect(screen.queryByTestId("suggestion-rail")).not.toBeInTheDocument(),
    );
  });

  it("marks the copy's changed lines in the read-only viewer", async () => {
    renderApp(
      <CodeEditor
        filePath="src/main.rs"
        content={"fn main() {\n    let x = 1;\n    return 1;\n}\n"}
        language="rust"
        editable={false}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
        changedLines={[{ start: 2, end: 3 }]}
      />,
    );
    const second = await screen.findByTestId("code-line-2");
    expect(second).toHaveAttribute("data-review-changed", "true");
    expect(screen.getByTestId("code-line-3")).toHaveAttribute(
      "data-review-changed",
      "true",
    );
    expect(screen.getByTestId("code-line-1")).not.toHaveAttribute(
      "data-review-changed",
    );
    expect(screen.getByTestId("code-line-4")).not.toHaveAttribute(
      "data-review-changed",
    );
  });
});
