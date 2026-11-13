import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createCommentAnchor } from "../../lib/commentAnchors";
import type { CommentDTO } from "../../lib/types";
import { SideThreads } from "./SideThreads";

vi.mock("../../hooks/useComments", () => ({
  useCommentReplies: () => ({ status: "ok", data: [] }),
}));

const content = "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\n";

function comment(
  id: string,
  createdAt: string,
  anchor: Partial<
    Pick<
      CommentDTO,
      "lineNumber" | "lineHash" | "contentHash" | "anchorContext"
    >
  >,
): CommentDTO {
  return {
    id,
    workspaceId: "ws",
    authorId: "user-a",
    authorName: "Alice",
    authorAvatarUrl: null,
    scope: "line",
    repoId: "repo",
    filePath: "docs/guide.md",
    lineNumber: null,
    lineHash: null,
    contentHash: null,
    anchorContext: null,
    commitSha: null,
    content: `Comment ${id}`,
    resolved: false,
    resolvedAt: null,
    originBranch: "main",
    replyCount: 0,
    createdAt,
    updatedAt: createdAt,
    ...anchor,
  };
}

/** The file's threads as the API lists them: newest first. */
async function threadsNewestFirst(): Promise<CommentDTO[]> {
  return [
    comment(
      "late-l8",
      "2026-10-30T10:30:00Z",
      await createCommentAnchor(content, 8),
    ),
    comment(
      "orphan",
      "2026-10-30T10:20:00Z",
      // Written on a line of text the file no longer holds anywhere.
      await createCommentAnchor("x\ny\nz\nvanished line\n", 4),
    ),
    comment("unanchored", "2026-10-30T10:10:00Z", {}),
    comment(
      "early-l3",
      "2026-10-30T10:00:00Z",
      await createCommentAnchor(content, 3),
    ),
  ];
}

function listedIds(panel: HTMLElement): string[] {
  return Array.from(
    panel.querySelectorAll<HTMLElement>("[data-comment-thread-id]"),
  ).map((el) => el.dataset.commentThreadId ?? "");
}

describe("SideThreads", () => {
  it("lists placed threads by line, then the unplaced in creation order", async () => {
    render(
      <SideThreads
        title="Comments on this file"
        testId="side-threads"
        comments={await threadsNewestFirst()}
        content={content}
        notice={null}
      >
        <p>The view</p>
      </SideThreads>,
    );
    const panel = screen.getByTestId("side-threads");
    await waitFor(() =>
      expect(listedIds(panel)).toEqual([
        "early-l3",
        "late-l8",
        "unanchored",
        "orphan",
      ]),
    );
    const anchorOf = (id: string) =>
      within(screen.getByTestId(`comment-thread-${id}`)).getByTestId(
        "comment-thread-anchor",
      );
    expect(anchorOf("early-l3")).toHaveTextContent("L3");
    expect(anchorOf("late-l8")).toHaveTextContent("L8");
    expect(anchorOf("unanchored")).toHaveTextContent("Not anchored to a line");
    expect(anchorOf("orphan")).toHaveTextContent("Orphaned anchor · was L4");
  });

  it("reveals a placed card's line when it is expanded and opens it from its label", async () => {
    const user = userEvent.setup();
    const onRevealLine = vi.fn();
    const onOpenLine = vi.fn();
    render(
      <SideThreads
        title="Comments on this file"
        testId="side-threads"
        comments={await threadsNewestFirst()}
        content={content}
        notice={null}
        onRevealLine={onRevealLine}
        onOpenLine={onOpenLine}
      >
        <p>The view</p>
      </SideThreads>,
    );
    const panel = screen.getByTestId("side-threads");
    await waitFor(() => expect(listedIds(panel)[0]).toBe("early-l3"));
    const card = (id: string) => screen.getByTestId(`comment-thread-${id}`);

    await user.click(
      within(card("early-l3")).getByTestId("comment-thread-toggle-expand"),
    );
    expect(onRevealLine).toHaveBeenCalledTimes(1);
    expect(onRevealLine).toHaveBeenCalledWith(3);
    // Collapsing reveals nothing.
    await user.click(
      within(card("early-l3")).getByTestId("comment-thread-toggle-expand"),
    );
    expect(onRevealLine).toHaveBeenCalledTimes(1);
    // An orphaned card has no line to reveal; its label is not a control.
    await user.click(
      within(card("orphan")).getByTestId("comment-thread-toggle-expand"),
    );
    expect(onRevealLine).toHaveBeenCalledTimes(1);
    expect(
      within(card("orphan")).getByTestId("comment-thread-anchor").tagName,
    ).toBe("SPAN");
    expect(
      within(card("unanchored")).getByTestId("comment-thread-anchor").tagName,
    ).toBe("SPAN");

    // A placed card's label opens its line in plain text without toggling the card.
    await user.click(
      within(card("late-l8")).getByRole("button", {
        name: "Open line 8 in plain text",
      }),
    );
    expect(onOpenLine).toHaveBeenCalledWith(8);
    expect(
      within(card("late-l8")).getByTestId("comment-thread-toggle-expand"),
    ).toHaveAttribute("aria-expanded", "false");
    expect(onRevealLine).toHaveBeenCalledTimes(1);
  });

  it("renders only the view while Comments is off", async () => {
    render(
      <SideThreads
        title="Comments on this file"
        testId="side-threads"
        shown={false}
        comments={await threadsNewestFirst()}
        content={content}
        notice={null}
      >
        <p>The view</p>
      </SideThreads>,
    );
    expect(screen.getByText("The view")).toBeInTheDocument();
    expect(screen.queryByTestId("side-threads")).toBeNull();
  });

  it("keeps the view in place when Comments turns on", async () => {
    const threads = await threadsNewestFirst();
    const ui = (shown: boolean) => (
      <SideThreads
        title="Comments on this file"
        testId="side-threads"
        shown={shown}
        comments={threads}
        content={content}
        notice={null}
      >
        <p>The view</p>
      </SideThreads>
    );
    const { rerender } = render(ui(false));
    const view = screen.getByText("The view");
    rerender(ui(true));
    expect(screen.getByTestId("side-threads")).toBeInTheDocument();
    // The same element: nothing of the view was remounted.
    expect(screen.getByText("The view")).toBe(view);
    rerender(ui(false));
    expect(screen.queryByTestId("side-threads")).toBeNull();
    expect(screen.getByText("The view")).toBe(view);
  });

  it("shows the list with its empty state and hint when Comments is on", () => {
    render(
      <SideThreads
        title="Comments on this file"
        testId="side-threads"
        comments={[]}
        content={content}
        notice={null}
        emptyHint="Select text to comment."
      >
        <p>The view</p>
      </SideThreads>,
    );
    const panel = screen.getByTestId("side-threads");
    expect(within(panel).getByTestId("side-threads-empty")).toHaveTextContent(
      "No comments on this file yet. Select text to comment.",
    );
  });

  it("writes a new thread first in the list and sends it with its quote", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn(async () => undefined);
    const onCancel = vi.fn();
    const subject = { kind: "draft" as const, id: "code:test" };
    const draft = { lineNumber: 3, quotedText: "gamma", text: content };
    render(
      <SideThreads
        title="Comments on this file"
        testId="side-threads"
        comments={await threadsNewestFirst()}
        content={content}
        notice={null}
        draft={{ subject, draft, closed: null, onSubmit, onCancel }}
      >
        <p>The view</p>
      </SideThreads>,
    );
    const panel = screen.getByTestId("side-threads");
    const card = within(panel).getByTestId("comment-draft-card");
    expect(screen.queryByTestId("side-threads-empty")).toBeNull();
    // The draft comes before every thread.
    await waitFor(() => expect(listedIds(panel)[0]).toBe("early-l3"));
    expect(
      card.compareDocumentPosition(
        screen.getByTestId("comment-thread-early-l3"),
      ) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await user.type(within(card).getByLabelText("Comment"), "Why gamma?");
    await user.click(within(card).getByTestId("draft-comment-save"));
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith("> gamma\n\nWhy gamma?"),
    );
    await user.click(within(card).getByTestId("draft-comment-cancel"));
    expect(onCancel).toHaveBeenCalled();
  });

  it("shows a closed draft's words without a way to send them", () => {
    const subject = { kind: "draft" as const, id: "code:test-closed" };
    render(
      <SideThreads
        title="Comments on this file"
        testId="side-threads"
        comments={[]}
        content={content}
        notice={null}
        draft={{
          subject,
          draft: { lineNumber: 3, quotedText: "", text: content },
          closed: "You can no longer start a thread here.",
          onSubmit: vi.fn(),
          onCancel: vi.fn(),
        }}
      >
        <p>The view</p>
      </SideThreads>,
    );
    expect(screen.getByTestId("draft-comment-closed")).toHaveTextContent(
      "You can no longer start a thread here.",
    );
    expect(screen.queryByTestId("draft-comment-save")).toBeNull();
  });
});
