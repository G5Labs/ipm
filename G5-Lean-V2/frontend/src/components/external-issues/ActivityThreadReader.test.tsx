import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { ActivityThreadReader } from "./ActivityThreadReader";

const parent = {
  id: "older-parent",
  workspaceId: "ws-1",
  authorId: "user-alex",
  authorName: "Alex",
  scope: "workspace",
  content: "Check missing allocations.",
  createdAt: "2026-10-25T10:00:00Z",
  resolved: false,
};

it("reads an older parent independently of Activity pages and retains its full replies", async () => {
  let parentReads = 0;
  server.use(
    http.get("/api/v1/workspaces/ws-1/comments", () => {
      parentReads += 1;
      return HttpResponse.json({ data: [parent] });
    }),
    http.get("/api/v1/comments/older-parent/replies", () =>
      HttpResponse.json({
        data: [
          {
            id: "r-1",
            authorName: "Jordan",
            content: "The workspace link remains available.",
          },
        ],
      }),
    ),
  );
  const close = vi.fn();
  const { queryClient } = renderApp(
    <ActivityThreadReader
      workspaceId="ws-1"
      commentId="older-parent"
      onClose={close}
    />,
  );
  // The reader shares the canonical reply cache used by the full thread.
  queryClient.setQueryData(
    ["workspace-review-replies", parent.id],
    [
      {
        id: "r-1",
        authorName: "Jordan",
        content: "The workspace link remains available.",
      },
    ],
  );
  const reader = await screen.findByTestId("activity-thread-reader");
  expect(reader).toHaveTextContent("Check missing allocations.");
  expect(reader).toHaveTextContent("The workspace link remains available.");
  expect(parentReads).toBe(1);
  await userEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(close).toHaveBeenCalledTimes(1);
});

it("does not substitute a different parent when the thread read fails and can retry", async () => {
  let unavailable = true;
  server.use(
    http.get("/api/v1/workspaces/ws-1/comments", () =>
      unavailable
        ? HttpResponse.json(
            { error: { code: "FORBIDDEN", message: "Not available" } },
            { status: 403 },
          )
        : HttpResponse.json({ data: [parent] }),
    ),
  );
  renderApp(
    <ActivityThreadReader
      workspaceId="ws-1"
      commentId={parent.id}
      onClose={vi.fn()}
    />,
  );
  const retry = await screen.findByTestId("activity-thread-retry");
  expect(screen.queryByTestId("activity-thread-reader")).toBeNull();
  unavailable = false;
  await userEvent.click(retry);
  await waitFor(() =>
    expect(
      within(screen.getByTestId("activity-thread-reader")).getByText(
        parent.content,
      ),
    ).toBeVisible(),
  );
});
