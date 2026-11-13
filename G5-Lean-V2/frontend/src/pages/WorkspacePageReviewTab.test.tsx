import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderApp } from "../test/render";
import { seedUserSettings } from "../test/userSettings";
import { server } from "../test/msw/server";
import { createMockUser } from "../test/fixtures";
import { WorkspacePage } from "./WorkspacePage";
import { requestReviewPush } from "../components/workspace/reviewWorkspace";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

const COPY_ID = "task-graph-rework";

function seedReviewCopy() {
  server.use(
    http.get("/api/v1/workspaces/:id/review", ({ params }) =>
      HttpResponse.json({
        data: {
          asSource: null,
          asReview:
            params.id === COPY_ID
              ? {
                  forkId: "fork-1",
                  orgId: "org-g5labs",
                  sourceWorkspaceId: "ws-source-1",
                  sourceWorkspaceName: "kaihui-test3",
                  reviewWorkspaceId: COPY_ID,
                  reviewWorkspaceName: "kaihui-test3-review-alex-chen",
                  reviewerUserId: "user-alex",
                  reviewerDisplayName: "Alex Chen",
                  vmStatus: "stopped",
                  vmStartedAt: null,
                  vmLastActivityAt: null,
                  lastFlushedAt: null,
                  createdAt: "2026-09-22T16:10:00.000Z",
                }
              : null,
          openSuggestions: 0,
        },
      }),
    ),
  );
}

function LocationProbe() {
  const location = useLocation();
  return (
    <div data-testid="location-probe">
      {location.pathname + location.search}
    </div>
  );
}

function renderCopy(route = `/org-g5labs/${COPY_ID}`) {
  return renderApp(
    <>
      <LocationProbe />
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>
    </>,
    { route, user: createMockUser() },
  );
}

describe("WorkspacePage on a review copy: the Review tab", () => {
  beforeEach(() => {
    // Tabs persist per workspace in localStorage: a Review tab saved by the
    // previous test would be restored into the next one and race its close.
    localStorage.clear();
    // The center tab bar is hidden by default (workspaces
    // g5.workspace.ui.center-tabs); this suite drives the tabs, so it opts in.
    seedUserSettings({ showTabBar: true });
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
    seedReviewCopy();
  });

  it("opens the Review tab by itself, with My review and the source's reviews inside it and nothing above the editor", async () => {
    renderCopy();
    const banner = await screen.findByTestId("workspace-review-banner");
    const tab = await screen.findByRole("tab", {
      name: /Review of kaihui-test3/,
    });
    await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
    const panel = document.getElementById("center-content-panel");
    expect(panel).not.toBeNull();
    await within(panel as HTMLElement).findByTestId("my-review-panel");
    within(panel as HTMLElement).getByTestId("review-rounds-overview");
    // One instance, inside the tab; the banner is a single line.
    expect(screen.getAllByTestId("my-review-panel")).toHaveLength(1);
    expect(within(banner).queryByTestId("my-review-panel")).toBeNull();
    expect(within(banner).queryByTestId("review-rounds-overview")).toBeNull();
  });

  it("cannot be closed: no close control on its tab, Close is disabled for it, and it sits first", async () => {
    renderCopy();
    const tab = await screen.findByRole("tab", {
      name: /Review of kaihui-test3/,
    });
    await screen.findByTestId("my-review-panel");
    await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
    const strip = tab.parentElement as HTMLElement;
    expect(within(strip).queryByTestId("center-tab-close")).toBeNull();
    // First content tab, right after Chat.
    const contentTabs = screen.getAllByTestId("center-content-tab");
    expect(contentTabs[0]).toBe(tab);
    // The context menu offers no way either.
    fireEvent.contextMenu(strip);
    const menu = await screen.findByTestId("tab-context-menu");
    const items = within(menu).getAllByTestId("center-tab-context-menu-item");
    expect(items[0]).toHaveTextContent("Close");
    expect(items[0]).toBeDisabled();
    // The panel is still there.
    expect(screen.getByTestId("my-review-panel")).toBeInTheDocument();
  });

  it("has no Approvals page: no View approvals, no Transfer", async () => {
    renderCopy();
    await screen.findByRole("tab", { name: /Review of kaihui-test3/ });
    await screen.findByTestId("workspace-review-banner");
    expect(screen.queryByText("View approvals")).toBeNull();
    expect(screen.queryByTestId("workspace-owner-transfer")).toBeNull();
    expect(screen.queryByRole("tab", { name: /^Approvals$/ })).toBeNull();
  });

  it("opens the Review tab for the ?view=approvals and ?view=reviews deep links", async () => {
    renderCopy(`/org-g5labs/${COPY_ID}?view=approvals`);
    const tab = await screen.findByRole("tab", {
      name: /Review of kaihui-test3/,
    });
    await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
    expect(screen.queryByRole("tab", { name: /^Approvals$/ })).toBeNull();
  });

  it("drops an Approvals tab restored from a previous visit", async () => {
    localStorage.setItem(
      `g5:tabs:${COPY_ID}`,
      JSON.stringify({
        tabs: [{ type: "workspace_approvals", name: "Approvals" }],
        activeTabId: "workspace-approvals",
      }),
    );
    try {
      renderCopy();
      await screen.findByRole("tab", { name: /Review of kaihui-test3/ });
      await waitFor(() =>
        expect(screen.queryByRole("tab", { name: /^Approvals$/ })).toBeNull(),
      );
    } finally {
      localStorage.removeItem(`g5:tabs:${COPY_ID}`);
    }
  });

  it("stays in the copy after a push and offers the source's reviews from the toast, on desktop too", async () => {
    server.use(
      http.post(`/api/v1/workspaces/${COPY_ID}/review/preview`, () =>
        HttpResponse.json({
          data: {
            baseCommitVector: [],
            files: [],
            skipped: [],
            saved: { workspace: "stopped", committed: false },
          },
        }),
      ),
      http.post(`/api/v1/workspaces/${COPY_ID}/review/publish`, () =>
        HttpResponse.json({
          data: {
            publication: {
              id: "pub-9",
              orgId: "org-g5labs",
              sourceWorkspaceId: "ws-source-1",
              reviewWorkspaceId: COPY_ID,
              reviewerUserId: "user-alex",
              authorKind: "human",
              authorLabel: "alex",
              round: 4,
              baseCommitVector: [],
              summary: "",
              verdict: "comment_only",
              scores: null,
              supersededBy: null,
              createdAt: "2026-09-27T10:00:00.000Z",
              kind: "round",
            },
            findings: [],
            suggestions: [],
            skipped: [],
          },
        }),
      ),
    );
    renderCopy();
    await screen.findByTestId("my-review-panel");
    // The desktop layout: the Chat surface is a tab, not a mobile sheet.
    expect(screen.getByTestId("center-chat-tab")).toBeInTheDocument();
    act(() => requestReviewPush());
    const dialog = await screen.findByTestId("push-review-dialog");
    await waitFor(() =>
      expect(within(dialog).getByTestId("push-review-confirm")).toBeEnabled(),
    );
    await userEvent.click(within(dialog).getByTestId("push-review-confirm"));
    const toast = await screen.findByTestId("toast");
    expect(toast).toHaveTextContent("Round 4 pushed");
    // Still on the copy: its Review tab is the page.
    expect(screen.getByTestId("my-review-panel")).toBeInTheDocument();
    await userEvent.click(
      within(toast).getByRole("button", { name: "Open on kaihui-test3" }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("location-probe")).toHaveTextContent(
        "/org-g5labs/kaihui-test3?view=reviews",
      ),
    );
  });

  it("opens from the ?view=review deep link", async () => {
    renderCopy(`/org-g5labs/${COPY_ID}?view=review`);
    const tab = await screen.findByRole("tab", {
      name: /Review of kaihui-test3/,
    });
    await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
    await screen.findByTestId("review-copy-page");
  });
});
