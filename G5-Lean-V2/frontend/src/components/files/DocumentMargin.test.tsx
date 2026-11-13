import React from "react";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { createCommentAnchor } from "../../lib/commentAnchors";
import type { CommentDTO, ReviewSuggestion } from "../../lib/types";
import {
  DocumentPreview,
  type DocumentMarginComments,
} from "./DocumentPreview";

vi.mock("../../hooks/useComments", () => ({
  useCommentReplies: () => ({ status: "ok", data: [] }),
}));

const content =
  "# Guide\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n";
const base = { filePath: "docs/guide.md", workspaceId: "ws", repoId: "repo" };

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

function suggestion(overrides: Partial<ReviewSuggestion>): ReviewSuggestion {
  return {
    id: "s1",
    publicationId: "publication",
    findingId: null,
    repoId: "repo",
    path: "docs/guide.md",
    kind: "hunk",
    beforeLines: "Third paragraph.\n",
    afterLines: "Third paragraph, revised.\n",
    anchorStart: 7,
    contentHash: null,
    anchorContext: null,
    authorKind: "human",
    authorLabel: "Reviewer",
    note: "",
    commentId: null,
    status: "open",
    statusBy: null,
    statusAt: null,
    supersedes: null,
    adoptedFrom: null,
    createdAt: "2026-10-30T12:00:00Z",
    ...overrides,
  };
}

const overlay = (suggestions: ReviewSuggestion[]) => ({
  workspaceId: "ws",
  suggestions,
  viewerOwnsWorkspace: true,
  viewerUserId: "owner",
  ownerVmRunning: true,
  sourceWritable: true,
});

function comments(
  threads: CommentDTO[],
  extra: Partial<DocumentMarginComments> = {},
): DocumentMarginComments {
  return {
    shown: true,
    threads,
    notice: null,
    emptyHint: "Select text to comment.",
    testId: "margin",
    ...extra,
  };
}

/**
 * jsdom lays nothing out. Each rendered block is given a top of 100 times its
 * index, every margin item a height of 40 and the row the given width.
 */
function fakeGeometry(rowWidth = 1200) {
  const rect = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    const block = this.closest("[data-block-index]");
    const index = block?.getAttribute("data-block-index") ?? null;
    // A block may be shifted by the test, as a changed preview shifts text.
    const shift = Number(block?.getAttribute("data-shift") ?? 0);
    const top = index === null ? 0 : 100 * Number(index) + shift;
    return {
      top,
      bottom: top,
      left: 0,
      right: 0,
      width: 0,
      height: 0,
      x: 0,
      y: top,
      toJSON: () => ({}),
    } as DOMRect;
  };
  const height = Object.getOwnPropertyDescriptor(
    HTMLElement.prototype,
    "offsetHeight",
  );
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return this.hasAttribute("data-margin-item") ? 40 : 0;
    },
  });
  const width = Object.getOwnPropertyDescriptor(
    Element.prototype,
    "clientWidth",
  );
  Object.defineProperty(Element.prototype, "clientWidth", {
    configurable: true,
    get(this: Element) {
      return this.getAttribute("data-testid") === "document-preview-row"
        ? rowWidth
        : 0;
    },
  });
  return () => {
    Element.prototype.getBoundingClientRect = rect;
    if (height)
      Object.defineProperty(HTMLElement.prototype, "offsetHeight", height);
    if (width) Object.defineProperty(Element.prototype, "clientWidth", width);
  };
}

/**
 * Waits for an item to settle at `top`. The document renders after the
 * margin's first measure; a resize makes it measure again, as in a browser
 * the document's size change does.
 */
const settled = (margin: HTMLElement, id: string, top: number) =>
  waitFor(() => {
    fireEvent(window, new Event("resize"));
    expect(topOf(margin, id)).toBe(top);
  });

const topOf = (margin: HTMLElement, id: string) =>
  parseFloat(
    (margin.querySelector(`[data-margin-item="${id}"]`) as HTMLElement).style
      .top,
  );

describe("the margin beside the rendered document", () => {
  let restore = () => {};
  beforeEach(() => {
    restore = fakeGeometry();
  });
  afterEach(() => restore());

  it("sets each thread level with its block, in the order of the text, and the expanded one first", async () => {
    const threads = [
      comment(
        "on-l7",
        "2026-10-30T10:30:00Z",
        await createCommentAnchor(content, 7),
      ),
      comment(
        "on-l3",
        "2026-10-30T10:00:00Z",
        await createCommentAnchor(content, 3),
      ),
      comment(
        "later-l3",
        "2026-10-30T10:10:00Z",
        await createCommentAnchor(content, 3),
      ),
      comment("unanchored", "2026-10-30T10:20:00Z", {}),
    ];
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        comments={comments(threads)}
      />,
    );
    const margin = await screen.findByTestId("margin");
    await settled(margin, "thread:on-l3", 100);
    // Two threads on one block: the earlier first, the later pushed below it.
    expect(topOf(margin, "thread:later-l3")).toBe(148);
    expect(topOf(margin, "thread:on-l7")).toBe(300);
    // A thread without a place follows the placed ones.
    expect(topOf(margin, "thread:unanchored")).toBe(348);
    expect(
      [...margin.querySelectorAll("[data-margin-item]")].map((el) =>
        el.getAttribute("data-margin-item"),
      ),
    ).toEqual([
      "thread:on-l3",
      "thread:later-l3",
      "thread:on-l7",
      "thread:unanchored",
    ]);
    // The document itself holds no card.
    expect(
      screen
        .getByTestId("document-preview-content")
        .querySelector("[data-comment-thread-id]"),
    ).toBeNull();
    // Expanding the pushed-down thread sets it level with its block again.
    fireEvent.click(
      within(
        margin.querySelector(
          '[data-margin-item="thread:later-l3"]',
        ) as HTMLElement,
      ).getByText("Comment later-l3"),
    );
    await settled(margin, "thread:later-l3", 100);
    expect(topOf(margin, "thread:on-l3")).toBe(52);
    expect(
      margin.querySelector('[data-margin-item="thread:later-l3"]'),
    ).toHaveAttribute("data-active", "true");
    // The document's content changes under the same size (a preview shows
    // another alternative): the margin measures again on its own.
    document
      .querySelector('[data-block-index="3"]')!
      .setAttribute("data-shift", "40");
    await waitFor(() => expect(topOf(margin, "thread:on-l7")).toBe(340));
  });

  it("shows suggestion cards with Comments off and threads only with it on", async () => {
    const threads = [
      comment(
        "t",
        "2026-10-30T10:00:00Z",
        await createCommentAnchor(content, 3),
      ),
    ];
    const view = renderApp(
      <DocumentPreview
        {...base}
        content={content}
        suggestionOverlay={overlay([suggestion({})])}
        comments={comments(threads, { shown: false })}
      />,
    );
    // With Comments off the margin is not the Comments panel.
    expect(screen.queryByTestId("margin")).toBeNull();
    const margin = await screen.findByTestId("document-margin");
    await within(margin).findByTestId("spec-suggestion-card-s1");
    expect(within(margin).queryByText("Comment t")).toBeNull();
    // The preview of the edit stays in the document, its card in the margin.
    expect(screen.getByTestId("document-preview-content")).toContainElement(
      screen.getByTestId("spec-suggestion-preview-s1"),
    );
    await settled(margin, "suggestion:s1", 300);
    // Comments on: the threads join the cards.
    view.unmount();
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        suggestionOverlay={overlay([suggestion({})])}
        comments={comments(threads)}
      />,
    );
    const shown = await screen.findByTestId("margin");
    await within(shown).findByText("Comment t");
    expect(
      within(shown).getByTestId("spec-suggestion-card-s1"),
    ).toBeInTheDocument();
  });

  it("sets a draft that just opened level with its line, ahead of the cards there", async () => {
    const threads = [
      comment(
        "a",
        "2026-10-30T10:00:00Z",
        await createCommentAnchor(content, 3),
      ),
      comment(
        "b",
        "2026-10-30T10:10:00Z",
        await createCommentAnchor(content, 3),
      ),
    ];
    const draft = {
      subject: { kind: "draft" as const, id: "code:test" },
      draft: { lineNumber: 3, quotedText: "First", text: content },
      closed: null,
      onSubmit: () => undefined,
      onCancel: () => undefined,
    };
    const view = renderApp(
      <DocumentPreview
        {...base}
        content={content}
        comments={comments(threads)}
      />,
    );
    const margin = await screen.findByTestId("margin");
    await settled(margin, "thread:b", 148);
    view.unmount();
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        comments={comments(threads, { draft })}
      />,
    );
    const withDraft = await screen.findByTestId("margin");
    await settled(withDraft, "comment-draft", 100);
    expect(
      withDraft.querySelector('[data-margin-item="comment-draft"]'),
    ).toHaveAttribute("data-active", "true");
    expect(topOf(withDraft, "thread:a")).toBe(148);
  });

  it("shows the empty state with its hint when Comments is on and there is nothing to list", async () => {
    renderApp(
      <DocumentPreview {...base} content={content} comments={comments([])} />,
    );
    expect(await screen.findByTestId("side-threads-empty")).toHaveTextContent(
      "No comments on this file yet. Select text to comment.",
    );
  });

  it("shows no margin while Comments is off and the file has no suggestions", async () => {
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        comments={comments([], { shown: false })}
      />,
    );
    await screen.findByTestId("document-preview");
    expect(screen.queryByTestId("margin")).toBeNull();
  });

  it("brings an activated card into view when the margin is stacked", async () => {
    restore();
    restore = fakeGeometry(390);
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        suggestionOverlay={overlay([suggestion({})])}
        comments={comments([], { shown: false })}
      />,
    );
    const margin = await screen.findByTestId("document-margin");
    await waitFor(() =>
      expect(margin).toHaveAttribute("data-layout", "stacked"),
    );
    const scrolled: Element[] = [];
    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      const mark = await screen.findByRole("button", {
        name: /Suggested (addition|removal)/,
      });
      fireEvent.click(mark);
      await waitFor(() => expect(scrolled.length).toBeGreaterThan(0));
      expect(scrolled[0]).toHaveAttribute("data-margin-item", "suggestion:s1");
    } finally {
      Element.prototype.scrollIntoView = scrollIntoView;
    }
  });

  it("stacks the cards below the document in a narrow row", async () => {
    restore();
    restore = fakeGeometry(390);
    const threads = [
      comment(
        "t",
        "2026-10-30T10:00:00Z",
        await createCommentAnchor(content, 3),
      ),
    ];
    renderApp(
      <DocumentPreview
        {...base}
        content={content}
        comments={comments(threads)}
      />,
    );
    const margin = await screen.findByTestId("margin");
    await waitFor(() =>
      expect(margin).toHaveAttribute("data-layout", "stacked"),
    );
    const item = margin.querySelector(
      '[data-margin-item="thread:t"]',
    ) as HTMLElement;
    expect(item.style.top).toBe("");
    // Listed under the document, an expanded card brings its block to the
    // top of the view instead of moving beside it.
    const scrolled: Element[] = [];
    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      await screen.findByText("First paragraph.");
      fireEvent.click(within(item).getByText("Comment t"));
      expect(scrolled).toHaveLength(1);
      expect(scrolled[0]).toHaveAttribute("data-block-index", "1");
    } finally {
      Element.prototype.scrollIntoView = scrollIntoView;
    }
  });
});
