/**
 * WorkspacePage — a cached name resolution that outlived its workspace.
 *
 * A workspace name can be reused once its workspace is deleted. When the
 * cached `/:org/:workspace` resolution still points at the deleted id (for
 * example, it was deleted in another tab), the page resolves the name again
 * once instead of polling the dead id behind "Failed to load workspace".
 */

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes, useNavigate } from "react-router-dom";
import { expect, it } from "vitest";
import { QueryClient } from "@tanstack/react-query";

import { resolveWorkspaceKey } from "../hooks/useWorkspace";
import { server } from "../test/msw/server";
import { _activeTestClients, renderApp } from "../test/render";
import { createMockUser } from "../test/fixtures";
import { WorkspacePage } from "./WorkspacePage";

const ORG = "org-g5labs";
const NAME = "issue-19";

function SwitchablePage() {
  const navigate = useNavigate();
  return (
    <>
      <button type="button" onClick={() => navigate(`/${ORG}/Issue-19`)}>
        Open Issue-19
      </button>
      <WorkspacePage />
    </>
  );
}

function renderWithStaleResolution(names: string[] = [NAME]) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  _activeTestClients.push(queryClient);
  for (const name of names) {
    queryClient.setQueryData(resolveWorkspaceKey(ORG, name), {
      orgId: ORG,
      workspaceId: "deleted-ws",
    });
  }
  renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<SwitchablePage />} />
    </Routes>,
    { route: `/${ORG}/${NAME}`, user: createMockUser(), queryClient },
  );
}

function serveDeletedWorkspace(currentId: string) {
  const resolves: string[] = [];
  const detailReads: string[] = [];
  server.use(
    http.get("/api/v1/resolve", ({ request }) => {
      resolves.push(new URL(request.url).searchParams.get("workspace") ?? "");
      return HttpResponse.json({
        data: { orgId: ORG, workspaceId: currentId },
      });
    }),
    http.get("/api/v1/workspaces/deleted-ws", () => {
      detailReads.push("deleted-ws");
      return HttpResponse.json(
        {
          error: {
            code: "NOT_FOUND",
            message: "Workspace not found",
            requestId: "",
          },
        },
        { status: 404 },
      );
    }),
    // Record the read, then fall through to the default detail handler.
    http.get("/api/v1/workspaces/task-graph-rework", () => {
      detailReads.push("task-graph-rework");
      return undefined;
    }),
  );
  return { resolves, detailReads };
}

it("resolves the name again when the cached id's workspace is gone", async () => {
  const { resolves, detailReads } = serveDeletedWorkspace("task-graph-rework");
  renderWithStaleResolution();

  await waitFor(() => expect(detailReads).toContain("task-graph-rework"));
  expect(resolves).toEqual([NAME]);
  await waitFor(() =>
    expect(screen.queryByText("Failed to load workspace")).toBeNull(),
  );
});

it("re-resolves only once when the name still points at a missing workspace", async () => {
  const { resolves } = serveDeletedWorkspace("deleted-ws");
  renderWithStaleResolution();

  expect(
    await screen.findByText("Failed to load workspace", undefined, {
      timeout: 5_000,
    }),
  ).toBeVisible();
  await new Promise((resolve) => setTimeout(resolve, 500));
  expect(resolves).toEqual([NAME]);
});

it("re-resolves each stale route name, even when they share the deleted id", async () => {
  const { resolves, detailReads } = serveDeletedWorkspace("task-graph-rework");
  renderWithStaleResolution([NAME, "Issue-19"]);
  const user = userEvent.setup();

  await waitFor(() => expect(detailReads).toContain("task-graph-rework"));
  await user.click(
    await screen.findByRole("button", { name: "Open Issue-19" }),
  );

  await waitFor(() => expect(resolves).toEqual([NAME, "Issue-19"]));
  await waitFor(() =>
    expect(screen.queryByText("Failed to load workspace")).toBeNull(),
  );
});
