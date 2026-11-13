import { beforeEach, it, expect, vi } from "vitest";
import { screen, waitFor, act, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes, useNavigate, useLocation } from "react-router-dom";
import { server } from "../test/msw/server";
import { renderApp } from "../test/render";
import { seedUserSettings } from "../test/userSettings";
import {
  createMockUser,
  createMockChatSession,
  createMockTask,
  mockWorkspaceDetails,
} from "../test/fixtures";
import { activateSidebarSectionAsync } from "../test/sidebar";
import { WorkspacePage } from "../pages/WorkspacePage";

// The center tab bar is hidden by default (workspaces
// g5.workspace.ui.center-tabs); this suite drives the tabs, so it opts in.
// The flows click the stacked sidebar, which is no longer the default
// (g5.ui.sidebar-default).
beforeEach(() =>
  seedUserSettings({ showTabBar: true, sidebarDisplayMode: "expanded" }),
);
function SwitchablePage({ destination }: { destination: string }) {
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <>
      <button onClick={() => navigate(destination)}>Switch workspace</button>
      <button onClick={() => navigate("/org-g5labs/task-graph-rework")}>
        Return to original
      </button>
      <WorkspacePage />
      <div data-testid="review-route">
        {location.pathname}
        {location.search}
      </div>
    </>
  );
}

it.each([false, true])(
  "scopes a delayed follow-up failure to its composer (new chat: %s)",
  async (openNewChat) => {
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
    const session = createMockChatSession({
      id: "late-failure",
      workspaceId: "task-graph-rework",
      scopeRef: null,
    });
    let release!: () => void;
    let received = false;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.get("/api/v1/workspaces/:id/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
      http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "No launch needed in this test",
            },
          },
          { status: 422 },
        ),
      ),
      http.get("/api/v1/workspaces/:id/sessions", () =>
        HttpResponse.json({ data: [session] }),
      ),
      http.get("/api/v1/sessions/:id", () =>
        HttpResponse.json({ data: session }),
      ),
      http.get("/api/v1/sessions/:id/messages", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/sessions/:id/task", () =>
        HttpResponse.json({ data: null }),
      ),
      http.post("/api/v1/sessions/:id/messages", async () => {
        received = true;
        await gate;
        return HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "Old chat mode mismatch",
            },
          },
          { status: 422 },
        );
      }),
    );
    const { queryClient } = renderApp(
      <Routes>
        <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
      </Routes>,
      {
        route: `/org-g5labs/task-graph-rework?session=${session.id}`,
        user: createMockUser(),
      },
    );
    const user = userEvent.setup();
    await screen.findByText("Not set");
    await waitFor(() =>
      expect(queryClient.getQueryData(["chat-session", session.id])).toEqual(
        session,
      ),
    );
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toBeEnabled());
    await user.type(input, "Continue");
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-send")).toBeEnabled(),
    );
    await user.click(await screen.findByTestId("chat-panel-send"));
    await waitFor(() => expect(received).toBe(true));
    if (openNewChat) await user.click(screen.getByTestId("new-session-header"));
    await act(async () => {
      release();
      await gate;
    });
    // Wait for the mutation to settle before checking the error's destination.
    await waitFor(() => expect(queryClient.isMutating()).toBe(0));
    await waitFor(() =>
      expect(screen.getByTestId("chat-input")).toHaveValue(
        openNewChat ? "" : "Continue",
      ),
    );
    if (openNewChat) {
      expect(
        screen.queryByText(/Old chat mode mismatch/),
      ).not.toBeInTheDocument();
    } else {
      expect(
        await screen.findAllByText(/Old chat mode mismatch/),
      ).not.toHaveLength(0);
    }
  },
);

it.each<{
  destination: string | null;
  returnToOriginal: boolean;
  replaceComposer?: boolean;
  rejectStart?: boolean;
}>([
  { destination: "/org-g5labs/Mainline", returnToOriginal: false },
  { destination: "/org-g5labs/auth-refactor", returnToOriginal: false },
  { destination: "/org-g5labs/Mainline", returnToOriginal: true },
  { destination: "/outside", returnToOriginal: false },
  { destination: null, returnToOriginal: false },
  { destination: null, returnToOriginal: false, replaceComposer: true },
  {
    destination: null,
    returnToOriginal: false,
    replaceComposer: true,
    rejectStart: true,
  },
])(
  "fences late Chat start for $destination, return: $returnToOriginal, replaced: $replaceComposer, rejected: $rejectStart",
  async ({ destination, returnToOriginal, replaceComposer, rejectStart }) => {
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
    let release!: () => void;
    let received = false;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const session = createMockChatSession({
      id: "review-late-session",
      workspaceId: "task-graph-rework",
      scopeType: "workspace",
      scopeRef: "task-graph-rework",
    });
    server.use(
      http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
        HttpResponse.json({
          data: {
            resolved: {
              schemaVersion: 1,
              resolutionId: "review-confirmed",
              configRevision: "test",
              catalogVersion: "test",
              provider: { value: "claude_code", provenance: "configured" },
              providerRoute: "anthropic_direct",
              model: { value: "configured-model", provenance: "configured" },
              upstreamModel: "configured-model",
              permissionMode: { value: "auto", provenance: "configured" },
              effort: { value: "high", provenance: "configured" },
              authMode: "anthropic_api_key",
              egressMode: "direct_provider",
              credentialReady: true,
              requiresInteractiveAuth: false,
            },
          },
        }),
      ),
      http.get("/api/v1/workspaces/:id/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
      http.get("/api/v1/sessions/review-late-session", () =>
        HttpResponse.json({ data: session }),
      ),
      http.get("/api/v1/sessions/review-late-session/messages", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/sessions/review-late-session/task", () =>
        HttpResponse.json({ data: null }),
      ),
      http.get("/api/v1/resolve", ({ request }) =>
        HttpResponse.json({
          data: {
            orgId: "org-g5labs",
            workspaceId:
              new URL(request.url).searchParams.get("workspace") === "Mainline"
                ? "base"
                : new URL(request.url).searchParams.get("workspace"),
          },
        }),
      ),
      http.get("/api/v1/workspaces/base", () =>
        HttpResponse.json({
          data: { ...mockWorkspaceDetails.base, name: "Mainline" },
        }),
      ),
      http.get("/api/v1/workspaces/task-graph-rework", () =>
        HttpResponse.json({
          data: { ...mockWorkspaceDetails["task-graph-rework"], isBase: false },
        }),
      ),
      http.post(
        "/api/v1/workspaces/task-graph-rework/sessions/start",
        async () => {
          received = true;
          await gate;
          if (rejectStart) {
            return HttpResponse.json(
              {
                error: {
                  code: "VALIDATION_ERROR",
                  message: "Old start failed",
                },
              },
              { status: 422 },
            );
          }
          return HttpResponse.json(
            {
              data: {
                session,
                taskId: "review-late-task",
                assistantMessageId: "review-late-message",
                message: {
                  messageId: "review-late-message",
                  inputId: "review-late-message",
                  status: "queued",
                },
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    // The replaced composer is the floating Chat opened from the corner
    // button, which is absent while Chat holds the center: with the tab
    // strip hidden the toolbar toggle leaves the center first.
    if (replaceComposer) seedUserSettings({ showTabBar: false });
    const { queryClient } = renderApp(
      <Routes>
        <Route
          path="/:org/:workspace/*"
          element={
            <SwitchablePage
              destination={destination ?? "/org-g5labs/task-graph-rework"}
            />
          }
        />
        <Route
          path="/outside"
          element={<div data-testid="review-route">/outside</div>}
        />
      </Routes>,
      { route: "/org-g5labs/task-graph-rework", user: createMockUser() },
    );
    const user = userEvent.setup();
    await user.type(
      await screen.findByTestId("chat-input"),
      "accepted working workspace turn",
    );
    await waitFor(() =>
      expect(screen.getByTestId("chat-panel-no-session-send")).toBeEnabled(),
    );
    await user.click(screen.getByTestId("chat-panel-no-session-send"));
    await waitFor(() => expect(received).toBe(true));
    if (replaceComposer) {
      // Leave the center only when Chat holds it (the toggle then closes).
      const toggle = screen.getByTestId("toggle-right-panel");
      if (toggle.getAttribute("title") === "Close chat") await user.click(toggle);
      await user.click(await screen.findByTestId("chat-corner-button"));
      await user.type(screen.getByTestId("chat-input"), "New composer draft");
    }
    if (destination) {
      await user.click(
        screen.getByRole("button", { name: "Switch workspace" }),
      );
      await waitFor(() =>
        expect(screen.getByTestId("review-route")).toHaveTextContent(
          destination,
        ),
      );
      if (destination.includes("Mainline")) {
        await waitFor(() =>
          expect(
            screen.getByTestId("workspace-readonly-banner"),
          ).toHaveTextContent(
            "Read-only workspace. Fork workspace to make edits.",
          ),
        );
        expect(screen.queryByTestId("center-chat-tab")).toBeNull();
      }
    }
    if (returnToOriginal) {
      await user.click(
        screen.getByRole("button", { name: "Return to original" }),
      );
      await screen.findByTestId("chat-input");
    }
    await act(async () => {
      release();
      await gate;
    });
    await waitFor(() => expect(queryClient.isMutating()).toBe(0));
    // A workspace route lands on the timeline (g5.workbench.landing); only
    // the fenced late start keeps its conversation hint.
    const expected = replaceComposer
      ? "/org-g5labs/task-graph-rework?view=activity"
      : !destination
        ? "/org-g5labs/task-graph-rework?session=review-late-session"
        : returnToOriginal
          ? "/org-g5labs/task-graph-rework?view=activity"
          : destination === "/outside"
            ? destination
            : `${destination}?view=activity`;
    await waitFor(() =>
      expect(screen.getByTestId("review-route").textContent).toBe(expected),
    );
    if (replaceComposer) {
      expect(screen.queryByText(/Old start failed/)).not.toBeInTheDocument();
      expect(screen.getByTestId("chat-input")).toHaveValue(
        "New composer draft",
      );
    }
    if (destination?.includes("Mainline") && !returnToOriginal) {
      expect(screen.queryByTestId("center-chat-tab")).toBeNull();
      expect(screen.queryByText(/Owner:|VM:/)).toBeNull();
    }
  },
);

it("ignores a delayed task Chat lookup after switching to Base", async () => {
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
  let release!: () => void;
  let taskReads = 0;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const task = {
    ...createMockTask({
      id: "late-task",
      name: "Deferred Chat lookup",
      workspaceId: "task-graph-rework",
      sessionId: null,
      taskType: "sync",
      status: "running",
    }),
  };
  server.use(
    http.get("/api/v1/resolve", ({ request }) =>
      HttpResponse.json({
        data: {
          orgId: "org-g5labs",
          workspaceId:
            new URL(request.url).searchParams.get("workspace") === "Mainline"
              ? "base"
              : "task-graph-rework",
        },
      }),
    ),
    http.get("/api/v1/workspaces/base", () =>
      HttpResponse.json({
        data: { ...mockWorkspaceDetails.base, name: "Mainline" },
      }),
    ),
    http.get("/api/v1/workspaces/:id/tasks", () =>
      HttpResponse.json({
        data: { tasks: [task], cursor: null, hasMore: false },
      }),
    ),
    http.get("/api/v1/tasks/late-task", async () => {
      taskReads += 1;
      await gate;
      return HttpResponse.json({
        data: { ...task, sessionId: "late-task-session" },
      });
    }),
  );
  renderApp(
    <Routes>
      <Route
        path="/:org/:workspace/*"
        element={<SwitchablePage destination="/org-g5labs/Mainline" />}
      />
    </Routes>,
    { route: "/org-g5labs/task-graph-rework", user: createMockUser() },
  );
  const user = userEvent.setup();
  await activateSidebarSectionAsync("sidebar-tasks-tab");
  await user.click(
    await within(screen.getByTestId("left-sidebar")).findByText(task.name),
  );
  await waitFor(() => expect(taskReads).toBeGreaterThanOrEqual(2));
  await user.click(screen.getByRole("button", { name: "Switch workspace" }));
  await screen.findByTestId("workspace-readonly-banner");
  await act(async () => {
    release();
    await new Promise((resolve) => setTimeout(resolve, 150));
  });
  expect(screen.getByTestId("review-route").textContent).toBe(
    "/org-g5labs/Mainline?view=activity",
  );
  expect(screen.queryByTestId("center-chat-tab")).toBeNull();
});

it("renames the open chat from the mobile header (g5.chat.ui.header-title)", async () => {
  // The default media mock matches no query, so the page uses MobileLayout.
  const session = createMockChatSession({
    id: "mobile-chat",
    workspaceId: "task-graph-rework",
    title: "Old name",
    scopeRef: null,
  });
  let renamed: unknown = null;
  server.use(
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/:id", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/:id/messages", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/sessions/:id/task", () =>
      HttpResponse.json({ data: null }),
    ),
    http.patch("/api/v1/sessions/:id", async ({ request }) => {
      renamed = await request.json();
      return HttpResponse.json({ data: { ...session, title: "New name" } });
    }),
  );
  renderApp(
    <Routes>
      <Route path="/:org/:workspace/*" element={<WorkspacePage />} />
    </Routes>,
    {
      route: `/org-g5labs/task-graph-rework?session=${session.id}`,
      user: createMockUser(),
    },
  );
  const user = userEvent.setup();

  await user.click(await screen.findByTestId("chat-header-rename"));
  const input = screen.getByTestId("chat-header-title-input");
  await user.clear(input);
  await user.type(input, "New name{Enter}");

  await waitFor(() => expect(renamed).toEqual({ title: "New name" }));
});
