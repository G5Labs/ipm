/**
 * useNotifications — Clear all (g5.user.notifications): the panel and the
 * bell share one unread set, so clearing empties both at once, and each
 * cleared row is marked read in the workspace that stores it, by id, so the
 * inbox stays cleared after a reload while a row that arrives after the
 * click stays unread.
 */

import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { NotificationPanel } from "../components/notifications/NotificationPanel";
import { server } from "../test/msw/server";
import { renderWithWs } from "../test/render";
import { useNotifications } from "./useNotifications";

const WS = "11111111-1111-4111-8111-111111111111";
const OTHER_WS = "22222222-2222-4222-8222-222222222222";
const USER = "33333333-3333-4333-8333-333333333333";

function row(
  id: string,
  workspaceId: string,
  title: string,
  readAt: string | null = null,
) {
  return {
    id,
    workspaceId,
    category: "review",
    severity: "info",
    title,
    message: "Open the review to see it.",
    taskId: null,
    commentId: null,
    replyId: null,
    actions: [],
    readAt,
    expiresAt: null,
    createdAt: "2026-10-31T08:00:00Z",
  };
}

/** The page's wiring: the bell counts and the panel lists `active`. */
function Probe() {
  const n = useNotifications(WS, USER);
  return (
    <>
      <span data-testid="bell-count">{n.active.length}</span>
      <NotificationPanel
        notifications={n.active}
        onAction={() => {}}
        onClearAll={n.clearAll}
        onClose={() => {}}
      />
    </>
  );
}

describe("useNotifications unread set", () => {
  it("keeps an unread pull request announcement in the panel while an old info toast expires", async () => {
    const anHourAgo = new Date(Date.now() - 3_600_000).toISOString();
    server.use(
      http.get(`/api/v1/workspaces/${WS}/notifications`, () =>
        HttpResponse.json({
          data: [
            {
              ...row("n-pr", WS, "PR #12 merged"),
              category: "pull_request",
              createdAt: anHourAgo,
            },
            {
              ...row("n-toast", WS, "Task completed"),
              category: "task_blocked",
              createdAt: anHourAgo,
            },
          ],
        }),
      ),
    );
    renderWithWs(<Probe />);

    // The announcement waits for the reader (g5.git.pr-merge-announcement);
    // the hour-old info toast is already expired.
    expect(await screen.findByText("PR #12 merged")).toBeInTheDocument();
    expect(screen.queryByText("Task completed")).toBeNull();

    // No dismissal timer fires for it: it is still there after the toast
    // expiry window the timer effect would have used.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 30));
    });
    expect(screen.getByText("PR #12 merged")).toBeInTheDocument();
    expect(screen.getByTestId("bell-count")).toHaveTextContent("1");
  });
});

describe("useNotifications Clear all", () => {
  it("empties the panel and the bell together and marks each row read where it is stored", async () => {
    const readAll: Array<{ workspace: string; ids: unknown }> = [];
    let own = [
      row("n-own", WS, "A round was pushed"),
      row("n-read", WS, "Already read", "2026-10-31T07:00:00Z"),
    ];
    server.use(
      http.get(`/api/v1/workspaces/${WS}/notifications`, () =>
        HttpResponse.json({ data: own }),
      ),
      http.post(
        "/api/v1/workspaces/:id/notifications/read-all",
        async ({ params, request }) => {
          const body = (await request.json()) as { ids?: string[] };
          readAll.push({ workspace: params.id as string, ids: body.ids });
          own = own.map((r) =>
            body.ids?.includes(r.id)
              ? { ...r, readAt: r.readAt ?? "2026-10-31T09:00:00Z" }
              : r,
          );
          return HttpResponse.json({ data: { updated: 1 } });
        },
      ),
    );
    const { ws, queryClient } = renderWithWs(<Probe />);
    // The other workspace's inbox is cached fresh, as after a recent visit.
    queryClient.setQueryData(["notifications", OTHER_WS], []);

    // A row the server already holds as read is not part of the unread set.
    expect(await screen.findByText("A round was pushed")).toBeInTheDocument();
    expect(screen.queryByText("Already read")).toBeNull();
    expect(screen.getByTestId("bell-count")).toHaveTextContent("1");

    // A review notice from another workspace arrives over the user channel.
    act(() => {
      ws.emit({
        channel: `user:${USER}`,
        event: "notification",
        payload: {
          workspaceId: OTHER_WS,
          notification: row(
            "n-other",
            OTHER_WS,
            "Zhengchun Liu accepted your review edit",
          ),
        },
      });
    });
    expect(
      await screen.findByText("Zhengchun Liu accepted your review edit"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("bell-count")).toHaveTextContent("2");

    fireEvent.click(screen.getByTestId("notification-panel-clear-all"));

    // Both empty at once, before any server answer.
    expect(screen.getByText("No notifications")).toBeInTheDocument();
    expect(screen.getByTestId("bell-count")).toHaveTextContent("0");

    // Persisted where each row lives, naming exactly the rows that were shown.
    await waitFor(() => {
      expect(readAll).toEqual(
        expect.arrayContaining([
          { workspace: WS, ids: ["n-own"] },
          { workspace: OTHER_WS, ids: ["n-other"] },
        ]),
      );
      expect(readAll).toHaveLength(2);
    });

    // The refetch after persisting keeps the inbox empty, and the other
    // workspace's cached inbox is invalidated too, so it refetches when seen.
    await waitFor(() =>
      expect(screen.getByText("No notifications")).toBeInTheDocument(),
    );
    expect(screen.queryByText("A round was pushed")).toBeNull();
    await waitFor(() =>
      expect(
        queryClient.getQueryState(["notifications", OTHER_WS])?.isInvalidated,
      ).toBe(true),
    );
  });
});
