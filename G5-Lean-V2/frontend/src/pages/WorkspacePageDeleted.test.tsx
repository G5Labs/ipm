/**
 * WorkspacePage — the routed workspace no longer exists.
 *
 * Deleting a workspace closes its tabs (g5.workbench.tabs.deleted). The page
 * learns of a deletion it did not perform from the server: the workspace it
 * shows reads 404 and the name no longer resolves (or still names the dead
 * id), so its tab closes as the page's own delete closes it, showing the
 * left neighbour, else Home. A route that never resolved — a stale link, or
 * a tab kept for a workspace deleted while the browser was away — keeps the
 * not-found page under a transient tab and leaves the list.
 */

import { screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation } from "react-router-dom";
import { expect, it } from "vitest";
import { QueryClient } from "@tanstack/react-query";

import { resolveWorkspaceKey } from "../hooks/useWorkspace";
import { WorkspaceTabsProvider } from "../hooks/useWorkspaceTabs";
import {
  readWorkspaceTabs,
  workspaceTabsKey,
  type WorkspaceTab,
} from "../lib/workspaceTabs";
import { server } from "../test/msw/server";
import { _activeTestClients, renderApp } from "../test/render";
import { createMockUser } from "../test/fixtures";
import { WorkspacePage } from "./WorkspacePage";

const USER = createMockUser();
const ORG = "org-g5labs";
const NAME = "issue-19";
const ALPHA: WorkspaceTab = { orgSlug: ORG, name: "alpha" };
const ISSUE: WorkspaceTab = { orgSlug: ORG, name: NAME };

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function seedTabs(tabs: WorkspaceTab[]) {
  localStorage.setItem(workspaceTabsKey(USER.id), JSON.stringify(tabs));
}

function renderAt(route: string, cachedResolution: string | null) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  _activeTestClients.push(queryClient);
  if (cachedResolution) {
    queryClient.setQueryData(resolveWorkspaceKey(ORG, NAME), {
      orgId: ORG,
      workspaceId: cachedResolution,
      workspaceName: NAME,
    });
  }
  renderApp(
    <WorkspaceTabsProvider>
      <LocationProbe />
      <Routes>
        <Route path="/:org/home" element={<div>home</div>} />
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>
    </WorkspaceTabsProvider>,
    { route, user: USER, queryClient },
  );
}

const NOT_FOUND = {
  error: { code: "NOT_FOUND", message: "Workspace not found", requestId: "" },
};

/** The name resolves to `currentId`, or to nothing when null. */
function serveResolution(currentId: string | null) {
  const resolves: string[] = [];
  server.use(
    http.get("/api/v1/resolve", ({ request }) => {
      const name = new URL(request.url).searchParams.get("workspace") ?? "";
      if (name !== NAME) return undefined;
      resolves.push(name);
      if (currentId === null) {
        return HttpResponse.json(NOT_FOUND, { status: 404 });
      }
      return HttpResponse.json({
        data: { orgId: ORG, workspaceId: currentId, workspaceName: NAME },
      });
    }),
    http.get("/api/v1/workspaces/deleted-ws", () =>
      HttpResponse.json(NOT_FOUND, { status: 404 }),
    ),
  );
  return resolves;
}

it("closes the tab of a deleted workspace and shows its left neighbour", async () => {
  localStorage.clear();
  seedTabs([ALPHA, ISSUE]);
  const resolves = serveResolution(null);
  renderAt(`/${ORG}/${NAME}`, "deleted-ws");

  await waitFor(() => expect(resolves).toEqual([NAME]));
  await waitFor(() =>
    expect(screen.getByTestId("location")).toHaveTextContent(`/${ORG}/alpha`),
  );
  expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA]);
});

it("closes the tab when the name still resolves to the deleted workspace, and goes Home without a neighbour", async () => {
  localStorage.clear();
  seedTabs([ISSUE]);
  const resolves = serveResolution("deleted-ws");
  renderAt(`/${ORG}/${NAME}`, "deleted-ws");

  await waitFor(() => expect(resolves).toEqual([NAME]));
  await waitFor(() =>
    expect(screen.getByTestId("location")).toHaveTextContent(`/${ORG}/home`),
  );
  expect(readWorkspaceTabs(USER.id)).toEqual([]);
});

it("keeps the tab when the name now belongs to a new workspace", async () => {
  localStorage.clear();
  seedTabs([ALPHA, ISSUE]);
  const resolves = serveResolution("task-graph-rework");
  renderAt(`/${ORG}/${NAME}`, "deleted-ws");

  await waitFor(() => expect(resolves).toEqual([NAME]));
  await waitFor(() =>
    expect(screen.queryByText("Failed to load workspace")).toBeNull(),
  );
  await new Promise((resolve) => setTimeout(resolve, 300));
  expect(screen.getByTestId("location")).toHaveTextContent(`/${ORG}/${NAME}`);
  expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA, ISSUE]);
});

it("drops the tab of a route that does not resolve and stays on the not-found page", async () => {
  localStorage.clear();
  seedTabs([ALPHA, ISSUE]);
  const resolves = serveResolution(null);
  renderAt(`/${ORG}/${NAME}`, null);

  await waitFor(() => expect(resolves).toEqual([NAME]));
  expect(await screen.findByText("Workspace not found")).toBeVisible();
  await waitFor(() => expect(readWorkspaceTabs(USER.id)).toEqual([ALPHA]));
  expect(screen.getByTestId("location")).toHaveTextContent(`/${ORG}/${NAME}`);
});
