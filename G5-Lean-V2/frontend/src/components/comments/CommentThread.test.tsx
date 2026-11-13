import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CommentThread } from "./CommentThread";

vi.mock("../../hooks/useComments", () => ({
  useCommentReplies: () => ({ status: "ok", data: [] }),
}));

const thread = {
  id: "thread-1",
  lineStart: 4,
  lineEnd: 4,
  resolved: false,
  comments: [
    {
      author: "Alice",
      avatar: "",
      text: "Please review this sentence.",
      time: "2026-08-04T00:00:00Z",
    },
  ],
};

describe("CommentThread replies", () => {
  beforeEach(() => vi.clearAllMocks());

  it("submits a nonblank reply through an explicit Send action without collapsing", async () => {
    const user = userEvent.setup();
    const onReply = vi.fn();
    render(<CommentThread thread={thread} onReply={onReply} />);

    await user.click(screen.getByTestId("comment-thread-thread-1"));
    const input = await screen.findByPlaceholderText("Reply…");
    const send = screen.getByRole("button", { name: "Send reply" });
    expect(send).toBeDisabled();

    await user.type(input, "  Looks good  ");
    expect(send).toBeEnabled();
    await user.click(send);

    await waitFor(() =>
      expect(onReply).toHaveBeenCalledWith("thread-1", "Looks good"),
    );
    await waitFor(() => expect(input).toHaveValue(""));
    expect(screen.getByPlaceholderText("Reply…")).toBeVisible();
  });

  it("also submits with Enter and keeps clicks inside the composer from toggling", async () => {
    const onReply = vi.fn();
    render(<CommentThread thread={thread} onReply={onReply} />);

    fireEvent.click(screen.getByTestId("comment-thread-thread-1"));
    const input = await screen.findByPlaceholderText("Reply…");
    fireEvent.click(input);
    expect(screen.getByPlaceholderText("Reply…")).toBeVisible();
    fireEvent.change(input, { target: { value: "Keyboard reply" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(onReply).toHaveBeenCalledWith("thread-1", "Keyboard reply"),
    );
    expect(await screen.findByPlaceholderText("Reply…")).toBeVisible();
  });

  it("expands from the keyboard-operable disclosure control", async () => {
    const user = userEvent.setup();
    render(<CommentThread thread={thread} onReply={vi.fn()} />);

    const toggle = screen.getByRole("button", { name: "Expand comment" });
    toggle.focus();
    await user.keyboard("{Enter}");

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(await screen.findByPlaceholderText("Reply…")).toBeVisible();
  });

  it("visually marks the thread selected from a source highlight", () => {
    render(<CommentThread thread={thread} active />);

    const card = screen.getByTestId("comment-thread-thread-1");
    expect(card).toHaveAttribute("data-active", "true");
    expect(card.className).toContain("shadow-[var(--g5-info)]");
  });

  it("uses a light success-green shadow for a resolved thread", () => {
    render(<CommentThread thread={{ ...thread, resolved: true }} active />);

    const card = screen.getByTestId("comment-thread-thread-1");
    expect(card).toHaveAttribute("data-resolved", "true");
    expect(card.className).toContain("bg-[var(--g5-ok)]/10");
    expect(card.className).toContain("shadow-[var(--g5-ok)]/20");
    expect(card.className).not.toContain("shadow-[var(--g5-info)]");
  });

  it("renders a persisted source quote as subdued italic context", () => {
    render(
      <CommentThread
        thread={{
          ...thread,
          comments: [
            {
              ...thread.comments[0],
              text: "> Selected source sentence\n> continues here\n\nPlease revise this wording.",
            },
          ],
        }}
      />,
    );

    const quote = screen.getByTestId("comment-thread-quote");
    expect(quote).toHaveTextContent("Selected source sentence continues here");
    expect(quote).toHaveClass("italic");
    expect(quote.className).toContain("text-[var(--g5-text2)]");
    expect(screen.getByText("Please revise this wording.")).toBeVisible();
  });

  it("confirms thread deletion before invoking the mutation", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    render(<CommentThread thread={thread} onDelete={onDelete} />);

    await user.click(screen.getByTestId("comment-thread-thread-1"));
    await user.click(screen.getByRole("button", { name: "Delete comment" }));
    const confirmation = screen.getByTestId("comment-delete-confirmation");
    expect(confirmation).toHaveTextContent("Delete this thread?");
    await user.click(
      within(confirmation).getByRole("button", { name: "Delete" }),
    );

    expect(onDelete).toHaveBeenCalledWith("thread-1");
  });

  it("offers only the actions whose handlers the caller passes", async () => {
    const user = userEvent.setup();
    render(<CommentThread thread={thread} onReply={vi.fn()} />);

    await user.click(screen.getByTestId("comment-thread-thread-1"));
    expect(screen.getByPlaceholderText("Reply…")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Resolve comment" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Delete comment" }),
    ).not.toBeInTheDocument();
  });

  it("shows a read-only thread without any composer or decision", async () => {
    const user = userEvent.setup();
    render(<CommentThread thread={thread} />);

    await user.click(screen.getByTestId("comment-thread-thread-1"));
    expect(screen.queryByPlaceholderText("Reply…")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Resolve comment" }),
    ).not.toBeInTheDocument();
  });

  it("keeps a refused reply's text and says why", async () => {
    const user = userEvent.setup();
    const onReply = vi
      .fn()
      .mockRejectedValue(new Error("Workspace is read-only"));
    render(<CommentThread thread={thread} onReply={onReply} />);

    await user.click(screen.getByTestId("comment-thread-thread-1"));
    const input = await screen.findByPlaceholderText("Reply…");
    await user.type(input, "Please keep me");
    await user.click(screen.getByRole("button", { name: "Send reply" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Workspace is read-only",
    );
    expect(input).toHaveValue("Please keep me");
  });

  it("sends a reply once while the first send is in flight", async () => {
    let finish: () => void = () => {};
    const onReply = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    render(<CommentThread thread={thread} onReply={onReply} />);

    fireEvent.click(screen.getByTestId("comment-thread-thread-1"));
    const input = await screen.findByPlaceholderText("Reply…");
    fireEvent.change(input, { target: { value: "Only once" } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onReply).toHaveBeenCalledTimes(1);
    finish();
    await waitFor(() => expect(input).toHaveValue(""));
  });

  it("shows how many replies a collapsed thread holds before they load", () => {
    render(<CommentThread thread={{ ...thread, replyCount: 3 }} />);
    expect(screen.getByText("3 more")).toBeVisible();
  });

  it("names the person who replied", async () => {
    render(
      <CommentThread
        expanded
        thread={{
          ...thread,
          comments: [
            ...thread.comments,
            {
              author: "Bob",
              avatar: "",
              text: "Renamed it.",
              time: "2026-08-04T01:00:00Z",
            },
          ],
        }}
      />,
    );
    expect(screen.getByText("Bob")).toBeVisible();
  });

  it("makes the line label open the line in plain text only for a placed thread", async () => {
    const user = userEvent.setup();
    const onOpenLine = vi.fn();
    const onToggle = vi.fn();
    const { rerender } = render(
      <CommentThread
        thread={{ ...thread, anchorStatus: "mapped" }}
        onOpenLine={onOpenLine}
        onToggle={onToggle}
      />,
    );
    const label = screen.getByRole("button", {
      name: "Open line 4 in plain text",
    });
    expect(label).toHaveAttribute("data-testid", "comment-thread-anchor");
    expect(label).toHaveTextContent("L4");
    await user.click(label);
    expect(onOpenLine).toHaveBeenCalledWith(4);
    expect(onToggle).not.toHaveBeenCalled();

    // Still being placed: the line it was written on opens.
    rerender(<CommentThread thread={thread} onOpenLine={onOpenLine} />);
    await user.click(
      screen.getByRole("button", { name: "Open line 4 in plain text" }),
    );
    expect(onOpenLine).toHaveBeenCalledTimes(2);

    // Unplaced threads keep a plain label.
    rerender(
      <CommentThread
        thread={{ ...thread, anchorStatus: "orphan", originalLine: 4 }}
        onOpenLine={onOpenLine}
      />,
    );
    expect(screen.queryByRole("button", { name: /in plain text/ })).toBeNull();
    expect(screen.getByTestId("comment-thread-anchor")).toHaveTextContent(
      "Orphaned anchor · was L4",
    );

    // Without the action (the Source margin), the label is text.
    rerender(<CommentThread thread={{ ...thread, anchorStatus: "mapped" }} />);
    expect(screen.queryByRole("button", { name: /in plain text/ })).toBeNull();
    expect(screen.getByTestId("comment-thread-anchor")).toHaveTextContent("L4");
  });

  it("shows an unplaced thread with the line it was written on", () => {
    render(
      <CommentThread
        thread={{
          ...thread,
          anchorStatus: "orphan",
          originalLine: 12,
          originalLineText: "## Deleted heading",
        }}
      />,
    );
    expect(screen.getByTestId("comment-thread-anchor")).toHaveTextContent(
      "Orphaned anchor · was L12",
    );
    expect(
      screen.getByTestId("comment-thread-original-line"),
    ).toHaveTextContent("## Deleted heading");
  });
});
