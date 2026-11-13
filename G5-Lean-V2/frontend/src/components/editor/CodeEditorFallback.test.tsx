// The editable fallback used when Monaco cannot load: review cards are placed
// in the text being edited, unsaved changes included
// (`g5.governance.review-suggestions.display`), and its comment margin lists
// threads, since its wrapped lines have no rows to sit beside
// (`g5.documents.comments`).

import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { createCommentAnchor, sha256Utf8 } from "../../lib/commentAnchors";
import type { CommentDTO, ReviewSuggestion } from "../../lib/types";
import { server } from "../../test/msw/server";
import { CodeEditor } from "./CodeEditor";

vi.mock("@monaco-editor/react", () => {
  throw new Error("Monaco is unavailable here");
});

const suggestion: ReviewSuggestion = {
  id: "sug-1",
  publicationId: "pub-1",
  findingId: null,
  repoId: "repo-1",
  path: "src/main.ts",
  kind: "comment",
  beforeLines: "second();\n",
  afterLines: null,
  anchorStart: 2,
  contentHash: null,
  anchorContext: null,
  authorKind: "human",
  authorLabel: "rita",
  note: "Consider renaming.",
  commentId: "thread-1",
  status: "open",
  statusBy: null,
  statusAt: null,
  supersedes: null,
  adoptedFrom: null,
  createdAt: "2026-09-30T10:00:00Z",
};

describe("CodeEditor fallback editor", () => {
  it("places review cards in the text being edited", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const { container } = render(
      <QueryClientProvider client={new QueryClient()}>
        <CodeEditor
          filePath="src/main.ts"
          content={"first();\nsecond();\n"}
          language="typescript"
          editable
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={{
            workspaceId: "ws-1",
            suggestions: [suggestion],
            viewerOwnsWorkspace: false,
            viewerUserId: "user-reader",
            ownerVmRunning: false,
          }}
        />
      </QueryClientProvider>,
    );
    const card = await screen.findByTestId("suggestion-fallback-sug-1");
    expect(card).toHaveTextContent("Line 2");
    const textarea = container.querySelector("textarea");
    if (!textarea) throw new Error("no fallback editor");

    // A line inserted above, unsaved: the card follows its line.
    fireEvent.change(textarea, {
      target: { value: "zero();\nfirst();\nsecond();\n" },
    });
    const moved = screen.getByTestId("suggestion-fallback-sug-1");
    expect(moved).toHaveTextContent("Line 3");
    expect(
      within(moved).getByTestId("suggestion-anchor-note-sug-1"),
    ).toHaveTextContent("Moved from line 2");

    // Its line deleted, unsaved: the card says so instead of matching.
    fireEvent.change(textarea, { target: { value: "zero();\nfirst();\n" } });
    expect(
      within(screen.getByTestId("suggestion-fallback-sug-1")).getByTestId(
        "suggestion-anchor-note-sug-1",
      ),
    ).toHaveTextContent("not in this file as shown");
  });

  it("lists the margin's threads in line order with the draft first", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const content = "first();\nsecond();\nthird();\n";
    const thread = async (
      id: string,
      lineNumber: number,
      overrides: Partial<CommentDTO> = {},
    ): Promise<CommentDTO> => {
      const anchor = await createCommentAnchor(content, lineNumber);
      return {
        id,
        workspaceId: "ws-1",
        authorId: "user-owner",
        authorName: "Olive Owner",
        authorAvatarUrl: null,
        scope: "line",
        repoId: "repo-1",
        filePath: "src/main.ts",
        lineNumber,
        lineHash: anchor.lineHash,
        contentHash: anchor.contentHash,
        anchorContext: anchor.anchorContext,
        commitSha: null,
        content: `On line ${lineNumber}`,
        resolved: false,
        resolvedAt: null,
        originBranch: "main",
        replyCount: 0,
        createdAt: "2026-09-30T10:00:00Z",
        updatedAt: "2026-09-30T10:00:00Z",
        ...overrides,
      };
    };
    const comments = [
      await thread("comment-lost", 1, {
        lineHash: await sha256Utf8("gone();"),
        anchorContext: null,
      }),
      await thread("comment-3", 3),
      await thread("comment-2", 2),
    ];
    const { container } = render(
      <QueryClientProvider client={new QueryClient()}>
        <CodeEditor
          filePath="src/main.ts"
          content={content}
          language="typescript"
          editable
          showComments
          onToggleComments={vi.fn()}
          comments={comments}
          onAddComment={vi.fn()}
        />
      </QueryClientProvider>,
    );
    const column = await screen.findByTestId("comment-column");
    await waitFor(() =>
      expect(
        within(column)
          .getAllByTestId(/^comment-thread-comment-/)
          .map((element) => element.dataset.testid),
      ).toEqual([
        "comment-thread-comment-2",
        "comment-thread-comment-3",
        "comment-thread-comment-lost",
      ]),
    );

    const textarea = container.querySelector("textarea");
    if (!textarea) throw new Error("no fallback editor");
    textarea.setSelectionRange(9, 9);
    fireEvent.select(textarea);
    fireEvent.contextMenu(textarea);
    fireEvent.click(
      screen.getByTestId("code-editor-context-add-comment-button"),
    );
    const draft = screen.getByLabelText("Comment");
    expect(
      draft.compareDocumentPosition(column) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("keeps a card's typed reply when the fallback editor gives way to the viewer and back", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    server.use(
      http.get(
        "*/api/v1/workspaces/ws-1/review/suggestions/sug-1/replies",
        () => HttpResponse.json({ data: { replies: [] } }),
      ),
    );
    const client = new QueryClient();
    const editor = (editable: boolean) => (
      <QueryClientProvider client={client}>
        <CodeEditor
          filePath="src/main.ts"
          content={"first();\nsecond();\n"}
          language="typescript"
          editable={editable}
          onSave={vi.fn()}
          showComments={false}
          onToggleComments={vi.fn()}
          comments={[]}
          suggestionOverlay={{
            workspaceId: "ws-1",
            suggestions: [suggestion],
            viewerOwnsWorkspace: false,
            viewerUserId: "user-reader",
            ownerVmRunning: false,
            sourceWritable: true,
          }}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(editor(true));
    const list = await screen.findByTestId("suggestion-fallback-sug-1");
    const user = userEvent.setup();
    await user.click(
      within(list).getByTestId("review-suggestion-replies-toggle-sug-1"),
    );
    const input = await within(list).findByTestId(
      "review-suggestion-reply-input-sug-1",
    );
    await user.type(input, "Keep me");

    // The workspace stops: the viewer shows the same card, with the words.
    rerender(editor(false));
    await waitFor(() =>
      expect(screen.getByTestId("suggestion-zone-after-2")).toContainElement(
        input,
      ),
    );
    expect(input).toHaveValue("Keep me");

    // It starts again, Monaco still unavailable: back in the list.
    rerender(editor(true));
    await waitFor(() =>
      expect(screen.getByTestId("suggestion-fallback-sug-1")).toContainElement(
        input,
      ),
    );
    expect(input).toHaveValue("Keep me");
  });
});
