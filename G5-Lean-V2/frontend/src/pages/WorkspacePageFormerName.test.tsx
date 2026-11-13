/**
 * WorkspacePage — a former name in the URL.
 *
 * A renamed workspace keeps every former name, and a URL naming one resolves
 * to it (g5.workspace.former-names). The page replaces the URL's workspace
 * segment with the current name, keeping the sub-route, query and fragment.
 */

import { screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation } from "react-router-dom";
import { expect, it } from "vitest";

import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { createMockUser } from "../test/fixtures";
import { WorkspacePage } from "./WorkspacePage";

const ORG = "org-g5labs";

function LocationProbe() {
  const location = useLocation();
  return (
    <div data-testid="location">
      {`${location.pathname}${location.search}${location.hash}`}
    </div>
  );
}

function serveRenamedWorkspace(currentName: string, workspaceId: string) {
  const resolves: string[] = [];
  server.use(
    http.get("/api/v1/resolve", ({ request }) => {
      resolves.push(new URL(request.url).searchParams.get("workspace") ?? "");
      return HttpResponse.json({
        data: { orgId: ORG, workspaceId, workspaceName: currentName },
      });
    }),
  );
  return resolves;
}

function renderAt(route: string) {
  renderApp(
    <>
      <LocationProbe />
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>
    </>,
    { route, user: createMockUser() },
  );
}

it("replaces a former name with the current name, keeping the query and fragment", async () => {
  const resolves = serveRenamedWorkspace("task-graph-rework", "task-graph-rework");
  renderAt(`/${ORG}/old-name?view=chat#top`);

  await waitFor(() =>
    expect(screen.getByTestId("location")).toHaveTextContent(
      `/${ORG}/task-graph-rework?view=chat#top`,
    ),
  );
  expect(resolves[0]).toBe("old-name");
});

it("leaves the URL alone when it already names the workspace", async () => {
  const resolves = serveRenamedWorkspace("task-graph-rework", "task-graph-rework");
  renderAt(`/${ORG}/task-graph-rework?view=chat`);

  await waitFor(() => expect(resolves).toContain("task-graph-rework"));
  await new Promise((resolve) => setTimeout(resolve, 200));
  expect(screen.getByTestId("location")).toHaveTextContent(
    `/${ORG}/task-graph-rework?view=chat`,
  );
});
