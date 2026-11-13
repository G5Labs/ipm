/**
 * Base's Show All PRs tab offers its row actions to an organization
 * administrator who does not own Base (`g5.governance.pr.base-admin`), and
 * still hides them from a plain member (`g5.workspace.ui.reader-publication`).
 */
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";
import { GETTING_STARTED_AUTO_OPENED_STORAGE_KEY } from "../lib/userGuide";
import {
  createMockUser,
  mockInstanceInfo,
  mockWorkspaceDetails,
} from "../test/fixtures";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { WorkspacePage } from "./WorkspacePage";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

const pullRequest = {
  id: "pr-into-base",
  sourceWorkspaceId: "ws-feature",
  sourceWorkspaceName: "Feature",
  targetWorkspaceId: "base",
  targetWorkspaceName: "Base",
  status: "open",
  localMergeStatus: "ready",
  providerStatus: "local_only",
  providerRepos: [],
  title: "Feature into Base",
  sourceHeads: [],
  targetHeads: [],
  preflight: {},
  lastPreflightFingerprint: "fingerprint",
  blockingMessage: null,
  createdAt: "2026-09-01T00:00:00Z",
  updatedAt: "2026-09-01T00:00:00Z",
  mergedAt: null,
  mergeAdmission: { admitted: true, reason: null, requirements: [] },
};

beforeEach(() => {
  localStorage.setItem(GETTING_STARTED_AUTO_OPENED_STORAGE_KEY, "1");
  vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
    matches: query.includes("min-width: 1024px"),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
  server.use(
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({ data: mockInstanceInfo }),
    ),
    http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/workspaces/:id", ({ params }) =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails[String(params.id)],
          isBase: true,
          ownerUserId: "user-somebody-else",
          readOnly: false,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id/pull-requests", ({ request }) =>
      HttpResponse.json({
        data:
          new URL(request.url).searchParams.get("role") === "target"
            ? [pullRequest]
            : [],
      }),
    ),
  );
});

function renderBaseAs(role: "admin" | "member") {
  const user = createMockUser();
  return renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    {
      route: "/org-g5labs/test-workspace?view=pull-requests",
      user: {
        ...user,
        orgs: user.orgs.map((membership) =>
          membership.orgId === "org-g5labs"
            ? { ...membership, role }
            : membership,
        ),
      },
    },
  );
}

// The Pull Requests surface opens from its `?view=` URL (g5.workbench.tabs).
async function openShowAllPrs() {
  await screen.findByText(pullRequest.title);
}

it("offers refresh, close and merge to an organization administrator who does not own Base", async () => {
  renderBaseAs("admin");
  await openShowAllPrs();
  await waitFor(() =>
    expect(
      screen.getByTestId(`pull-request-merge-button-${pullRequest.id}`),
    ).toBeEnabled(),
  );
  expect(
    screen.getByTestId(`pull-request-close-button-${pullRequest.id}`),
  ).toBeEnabled();
  expect(
    screen.getByTestId(`pull-request-refresh-button-${pullRequest.id}`),
  ).toBeEnabled();
});

it("hides the row actions from a member who neither owns nor administers Base", async () => {
  renderBaseAs("member");
  await openShowAllPrs();
  expect(
    screen.queryByTestId(`pull-request-merge-button-${pullRequest.id}`),
  ).toBeNull();
  expect(
    screen.queryByTestId(`pull-request-close-button-${pullRequest.id}`),
  ).toBeNull();
  expect(
    screen.queryByTestId(`pull-request-refresh-button-${pullRequest.id}`),
  ).toBeNull();
});
