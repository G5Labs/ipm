import { screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../test/render";
import { server } from "../test/msw/server";
import { createMockUser } from "../test/fixtures";
import { WorkspacePage } from "./WorkspacePage";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

describe("WorkspacePage on a review copy", () => {
  it("counts the file badges from the source workspace, where the publications live", async () => {
    vi.mocked(window.matchMedia).mockImplementation((query) => ({
      matches: query.includes("min-width: 1024px"),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
    const workspaceId = "task-graph-rework";
    const countedFor: string[] = [];
    server.use(
      http.get("/api/v1/workspaces/:id/review", ({ params }) =>
        HttpResponse.json({
          data: {
            asSource: null,
            asReview:
              params.id === workspaceId
                ? {
                    forkId: "fork-1",
                    orgId: "org-g5labs",
                    sourceWorkspaceId: "ws-source-1",
                    sourceWorkspaceName: "kaihui-test3",
                    reviewWorkspaceId: workspaceId,
                    reviewWorkspaceName: "kaihui-test3-review-alex-chen",
                    reviewerUserId: "user-alex",
                    reviewerDisplayName: "Alex Chen",
                    vmStatus: "running",
                    vmStartedAt: "2026-09-23T14:02:00.000Z",
                    vmLastActivityAt: "2026-09-23T14:20:00.000Z",
                    lastFlushedAt: null,
                    createdAt: "2026-09-22T16:10:00.000Z",
                  }
                : null,
            openSuggestions: 0,
          },
        }),
      ),
      http.get(
        "/api/v1/workspaces/:id/review/suggestions/counts",
        ({ params }) => {
          countedFor.push(String(params.id));
          return HttpResponse.json({ data: { paths: [] } });
        },
      ),
    );
    renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      { route: `/org-g5labs/${workspaceId}`, user: createMockUser() },
    );
    await screen.findByTestId("workspace-review-banner");
    // Once the page knows it is a copy, the badges are the source's.
    await waitFor(() => expect(countedFor).toContain("ws-source-1"));
    expect(countedFor[countedFor.length - 1]).toBe("ws-source-1");
  });
});
