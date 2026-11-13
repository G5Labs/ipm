import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import { CommentColumn } from "./CommentColumn";

vi.mock("../../hooks/useComments", () => ({
  useCommentReplies: () => ({ status: "ok", data: [] }),
}));

describe("CommentColumn", () => {
  it("wires the resolved-thread return action back to the caller", async () => {
    const user = userEvent.setup();
    const onUnresolve = vi.fn();
    const props = {
      threads: [
        {
          id: "resolved-thread",
          lineStart: 3,
          lineEnd: 3,
          resolved: true,
          comments: [
            {
              author: "Alice",
              avatar: "",
              text: "Resolved comment",
              time: "2026-08-04T00:00:00Z",
            },
          ],
        },
      ],
      onUnresolve,
    } as ComponentProps<typeof CommentColumn> & {
      onUnresolve: (threadId: string) => void;
    };

    render(<CommentColumn {...props} />);
    await user.click(screen.getByTestId("comment-thread-resolved-thread"));
    await user.click(
      await screen.findByRole("button", { name: "Reopen comment" }),
    );

    await waitFor(() =>
      expect(onUnresolve).toHaveBeenCalledWith("resolved-thread"),
    );
  });

  it("keeps only the selected thread expanded", async () => {
    const user = userEvent.setup();
    render(
      <CommentColumn
        onReply={vi.fn()}
        threads={[
          {
            id: "first-thread",
            lineStart: 1,
            lineEnd: 1,
            resolved: false,
            comments: [
              {
                author: "Alice",
                avatar: "",
                text: "First comment",
                time: "2026-08-04T00:00:00Z",
              },
            ],
          },
          {
            id: "second-thread",
            lineStart: 2,
            lineEnd: 2,
            resolved: false,
            comments: [
              {
                author: "Bob",
                avatar: "",
                text: "Second comment",
                time: "2026-08-04T00:01:00Z",
              },
            ],
          },
        ]}
      />,
    );

    const first = screen.getByTestId("comment-thread-first-thread");
    const second = screen.getByTestId("comment-thread-second-thread");
    await user.click(first);
    expect(await within(first).findByPlaceholderText("Reply…")).toBeVisible();

    await user.click(second);

    await waitFor(() =>
      expect(within(first).queryByPlaceholderText("Reply…")).toBeNull(),
    );
    expect(await within(second).findByPlaceholderText("Reply…")).toBeVisible();
  });

  it("lets local selection move away from an externally activated thread", async () => {
    const user = userEvent.setup();
    const threads = ["first-thread", "second-thread"].map((id, index) => ({
      id,
      lineStart: index + 1,
      lineEnd: index + 1,
      resolved: false,
      comments: [
        {
          author: "Alice",
          avatar: "",
          text: `${index === 0 ? "First" : "Second"} comment`,
          time: "2026-08-04T00:00:00Z",
        },
      ],
    }));
    render(
      <CommentColumn
        onReply={vi.fn()}
        threads={threads}
        activeThreadId="first-thread"
      />,
    );

    const first = screen.getByTestId("comment-thread-first-thread");
    const second = screen.getByTestId("comment-thread-second-thread");
    expect(await within(first).findByPlaceholderText("Reply…")).toBeVisible();
    await user.click(second);
    await waitFor(() =>
      expect(within(first).queryByPlaceholderText("Reply…")).toBeNull(),
    );
    expect(await within(second).findByPlaceholderText("Reply…")).toBeVisible();
  });
});
