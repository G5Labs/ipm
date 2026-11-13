// =============================================================================
// WorkspacePage — personal workspace routes and presentation (#624)
// =============================================================================
// `/:org/me` and `/:org/users/:handle` resolve the workspace by registry kind
// through their own endpoints, never the name resolver
// (personal_workspace g5.personal.routes). The page then shows the personal
// presentation chosen by `kind` (g5.personal.workbench): no phases, ribbon
// actions, publication, PRs or fork; the admin-visibility notice
// (g5.personal.access); and, while the VM is stopped, a composer that offers
// Start and send with `startCompute: true` (g5.personal.start-and-send).
// =============================================================================

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Link, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { QueryClient } from "@tanstack/react-query";

import { MOCK_PERSONAL_WORKSPACE_DETAIL } from "../data/mock";
import { resolveWorkspaceKey } from "../hooks/useWorkspace";
import { WorkspaceTabsProvider } from "../hooks/useWorkspaceTabs";
import {
  WORKSPACE_SHELL_ROUTE_PATTERNS,
  PERSONAL_SHELL_ROUTE_PATTERNS,
} from "../lib/shellRoutes";
import { readWorkspaceTabs, workspaceTabsKey } from "../lib/workspaceTabs";
import {
  createMockChatSession,
  createMockUser,
  createMockWorkspaceVm,
  mockWorkspaceDetails,
} from "../test/fixtures";
import { server } from "../test/msw/server";
import { _activeTestClients, renderApp } from "../test/render";
import { WorkspacePage } from "./WorkspacePage";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div data-testid="terminal-panel-stub" />,
}));

beforeEach(() => {
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
});

afterEach(() => server.events.removeAllListeners("request:start"));

beforeEach(() => {
  server.use(
    http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
      HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
  );
});

const RESOLVED_LAUNCH = {
  schemaVersion: 1,
  resolutionId: "personal-confirmed",
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
};

/** Record which resolution endpoints the page called. */
function recordResolutions() {
  const calls: string[] = [];
  server.events.on("request:start", ({ request }) => {
    const path = new URL(request.url).pathname;
    if (
      path === "/api/v1/resolve" ||
      path.endsWith("/me") ||
      path.endsWith("/workspace")
    ) {
      calls.push(path);
    }
  });
  return calls;
}

function serveVm(status: "stopped" | "running" | "starting") {
  server.use(
    http.get("/api/v1/workspaces/:id/vm", ({ params }) =>
      HttpResponse.json({
        data: createMockWorkspaceVm({
          orgId: "org-g5labs",
          workspaceId: String(params.id),
          ownerUserId: "user-alex",
          status,
          vmId: status === "running" ? `vm-${String(params.id)}` : null,
        }),
      }),
    ),
  );
}

function LocationEcho() {
  const location = useLocation();
  return (
    <div data-testid="location">
      {location.pathname}
      {location.search}
    </div>
  );
}

function renderRoute(
  route: string,
  user = createMockUser(),
  queryClient?: QueryClient,
  options: { nameLink?: string } = {},
) {
  return renderApp(
    <WorkspaceTabsProvider>
      <LocationEcho />
      {options.nameLink && (
        <Link data-testid="name-link" to={options.nameLink}>
          name link
        </Link>
      )}
      <Routes>
        {PERSONAL_SHELL_ROUTE_PATTERNS.map(({ family, path }) => (
          <Route
            key={path}
            path={path}
            element={<WorkspacePage key={family} routeFamily={family} />}
          />
        ))}
        {WORKSPACE_SHELL_ROUTE_PATTERNS.map((path) => (
          <Route key={path} path={path} element={<WorkspacePage />} />
        ))}
      </Routes>
    </WorkspaceTabsProvider>,
    { route, user, queryClient },
  );
}

/**
 * A query client that already holds the personal workspace's resolution and
 * detail, as after an earlier visit: the page then redirects from its first
 * render, before the strip has recorded the arrival.
 */
function warmPersonalCache(): QueryClient {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  _activeTestClients.push(queryClient);
  queryClient.setQueryData(resolveWorkspaceKey("g5labs", "users/alex"), {
    orgId: "org-g5labs",
    workspaceId: MOCK_PERSONAL_WORKSPACE_DETAIL.id,
    workspaceName: "users/alex",
  });
  queryClient.setQueryData(
    ["workspace", MOCK_PERSONAL_WORKSPACE_DETAIL.id],
    MOCK_PERSONAL_WORKSPACE_DETAIL,
  );
  return queryClient;
}

describe("personal workspace routes (g5.personal.routes)", () => {
  it("/:org/me resolves through GET /orgs/:orgId/me, never the name resolver", async () => {
    serveVm("running");
    const calls = recordResolutions();
    renderRoute("/g5labs/me");

    const notice = await screen.findByTestId("personal-workspace-notice");
    expect(notice).toHaveTextContent("Your personal workspace.");
    expect(notice).toHaveTextContent(
      "Organization administrators can see your personal work.",
    );
    expect(calls).toContain("/api/v1/orgs/org-g5labs/me");
    expect(calls).not.toContain("/api/v1/resolve");
  });

  it("/:org/users/:handle resolves by handle for an administrator and names the owner", async () => {
    serveVm("running");
    const calls = recordResolutions();
    renderRoute(
      "/g5labs/users/alex",
      createMockUser({ id: "user-jordan", email: "jordan@g5labs.dev" }),
    );

    const notice = await screen.findByTestId("personal-workspace-notice");
    await waitFor(() =>
      expect(notice).toHaveTextContent("Alex Chen's personal workspace."),
    );
    expect(notice).toHaveTextContent("as an organization administrator");
    expect(calls).toContain("/api/v1/orgs/org-g5labs/users/alex/workspace");
    expect(calls).not.toContain("/api/v1/resolve");
  });

  it("reports an unknown or unreadable handle as a personal workspace not found", async () => {
    renderRoute("/g5labs/users/bob");
    expect(
      await screen.findByText("Personal workspace not found"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("workspace-not-found-message")).toHaveTextContent(
      "Only its owner and organization administrators can open it.",
    );
  });

  it("tells a viewer refused at /:org/me that viewers have none", async () => {
    server.use(
      http.get("/api/v1/orgs/:orgId/me", () =>
        HttpResponse.json(
          { error: { code: "FORBIDDEN", message: "Viewers have none" } },
          { status: 403 },
        ),
      ),
    );
    renderRoute("/g5labs/me");
    expect(
      await screen.findByText("No personal workspace"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("workspace-not-found-message")).toHaveTextContent(
      "Viewers have no personal workspace in this organization.",
    );
  });
});

describe("personal workbench presentation (g5.personal.workbench)", () => {
  it("removes phases and ribbon actions, by kind", async () => {
    serveVm("running");
    renderRoute("/g5labs/me");

    const ribbon = await screen.findByTestId("action-ribbon");
    expect(screen.queryByTestId("phase-chevrons")).toBeNull();
    expect(within(ribbon).queryAllByRole("group")).toHaveLength(0);
    expect(screen.getByTestId("personal-workspace-notice")).toBeVisible();
    // Transfer is never offered (g5.personal.refusals).
    expect(within(ribbon).queryByTestId("workspace-owner-transfer")).toBeNull();
  });

  it("drops deep links to surfaces a personal workspace does not offer", async () => {
    serveVm("running");
    for (const view of ["pull-requests", "approvals"]) {
      const { unmount } = renderRoute(`/g5labs/me?view=${view}`);
      await waitFor(() =>
        expect(screen.getByTestId("location").textContent).toBe("/g5labs/me"),
      );
      expect(screen.queryByTestId("pull-request-panel")).toBeNull();
      unmount();
    }
  });

  it.each(["progress", "unavailable-view"])(
    "uses the Workspace fallback for an unavailable %s deep link",
    async (view) => {
      serveVm("running");
      renderRoute(`/g5labs/me?view=${view}`);

      expect(
        await screen.findByTestId("personal-workspace-notice"),
      ).toBeVisible();
      await waitFor(() =>
        expect(screen.getByTestId("location").textContent).toBe(
          "/g5labs/me?view=activity",
        ),
      );
      expect(screen.queryByRole("tab", { name: /Progress/ })).toBeNull();
    },
  );

  it("keeps Download code on Workspace Details", async () => {
    serveVm("running");
    renderRoute("/g5labs/me?view=details");

    const panel = await screen.findByTestId("workspace-details-panel");
    expect(
      await within(panel).findByTestId("workspace-personal-badge"),
    ).toBeVisible();
    // Download stays: files are kept.
    expect(
      within(panel).getByTestId("workspace-download-source"),
    ).toBeVisible();
  });

  // The arrival on the name route opens a name tab; the redirect moves that
  // tab onto the personal route rather than opening a second one (#947).
  it("moves a name link to a personal workspace onto its kind-resolved route, and its tab with it", async () => {
    serveVm("running");
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({
          data: {
            orgId: "org-g5labs",
            workspaceId: MOCK_PERSONAL_WORKSPACE_DETAIL.id,
          },
        }),
      ),
    );
    const user = createMockUser();
    renderRoute("/g5labs/users%2Falex/file/notes.md?view=details", user);
    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent(
        "/g5labs/me/file/notes.md?view=details",
      ),
    );
    await waitFor(() =>
      expect(readWorkspaceTabs(user.id)).toEqual([
        { orgSlug: "g5labs", name: "me", personal: "me" },
      ]),
    );
  });

  // The issue's own sequence: the personal workspace is open on its kind
  // route and its resolution cached; a name link to it is then followed in
  // the app. The page redirects from its first render, before the strip
  // records the name arrival, and that arrival must not reopen the name tab.
  it("keeps one tab when a cached name link redirects on first render", async () => {
    serveVm("running");
    const admin = createMockUser({
      id: "user-jordan",
      email: "jordan@g5labs.dev",
    });
    const member = { orgSlug: "g5labs", name: "users/alex", personal: "user" };
    renderRoute("/g5labs/users/alex", admin, warmPersonalCache(), {
      nameLink: "/g5labs/users%2Falex?view=activity",
    });
    await waitFor(() => expect(readWorkspaceTabs(admin.id)).toEqual([member]));

    await userEvent.click(screen.getByTestId("name-link"));
    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent(
        "/g5labs/users/alex?view=activity",
      ),
    );
    // The name tab is not added by the redirected arrival, early or late.
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(readWorkspaceTabs(admin.id)).toEqual([member]);
  });

  // The same, with the name tab already stored from before: the redirect
  // merges it into the personal tab, and the arrival must not add it back.
  it("merges a stored name tab into the personal tab on a cached redirect", async () => {
    serveVm("running");
    const admin = createMockUser({
      id: "user-jordan",
      email: "jordan@g5labs.dev",
    });
    const named = { orgSlug: "g5labs", name: "users/alex" };
    const member = { ...named, personal: "user" };
    localStorage.setItem(workspaceTabsKey(admin.id), JSON.stringify([named]));
    renderRoute("/g5labs/users/alex", admin, warmPersonalCache(), {
      nameLink: "/g5labs/users%2Falex?view=activity",
    });
    await waitFor(() =>
      expect(readWorkspaceTabs(admin.id)).toEqual([named, member]),
    );

    await userEvent.click(screen.getByTestId("name-link"));
    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent(
        "/g5labs/users/alex?view=activity",
      ),
    );
    await waitFor(() => expect(readWorkspaceTabs(admin.id)).toEqual([member]));
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(readWorkspaceTabs(admin.id)).toEqual([member]);
  });

  it("moves a name link to a member's personal workspace onto /:org/users/:handle as one tab", async () => {
    serveVm("running");
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({
          data: {
            orgId: "org-g5labs",
            workspaceId: MOCK_PERSONAL_WORKSPACE_DETAIL.id,
          },
        }),
      ),
    );
    const admin = createMockUser({
      id: "user-jordan",
      email: "jordan@g5labs.dev",
    });
    renderRoute("/g5labs/users%2Falex", admin);
    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent(
        "/g5labs/users/alex",
      ),
    );
    await waitFor(() =>
      expect(readWorkspaceTabs(admin.id)).toEqual([
        { orgSlug: "g5labs", name: "users/alex", personal: "user" },
      ]),
    );
  });

  it("keeps an ordinary workspace named like a personal one ordinary", async () => {
    serveVm("running");
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({
          data: { orgId: "org-g5labs", workspaceId: "task-graph-rework" },
        }),
      ),
      http.get("/api/v1/workspaces/task-graph-rework", () =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails["task-graph-rework"],
            name: "users/lookalike",
            kind: "working",
          },
        }),
      ),
    );
    renderRoute("/g5labs/users%2Flookalike");
    await screen.findByTestId("phase-chevrons");
    expect(screen.queryByTestId("personal-workspace-notice")).toBeNull();
  });
});

describe("Start and send in the personal workbench (g5.personal.start-and-send)", () => {
  function serveChatStart() {
    const starts: Record<string, unknown>[] = [];
    const session = createMockChatSession({
      id: "personal-session",
      workspaceId: MOCK_PERSONAL_WORKSPACE_DETAIL.id,
      scopeType: "workspace",
      scopeRef: MOCK_PERSONAL_WORKSPACE_DETAIL.id,
    });
    server.use(
      http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
        HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } }),
      ),
      http.get("/api/v1/workspaces/:id/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
      http.get("/api/v1/sessions/personal-session", () =>
        HttpResponse.json({ data: session }),
      ),
      http.get("/api/v1/sessions/personal-session/messages", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/sessions/personal-session/task", () =>
        HttpResponse.json({ data: null }),
      ),
      http.post(
        "/api/v1/workspaces/:id/sessions/start",
        async ({ request }) => {
          starts.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            {
              data: {
                session,
                taskId: "personal-task",
                assistantMessageId: "personal-message",
                message: {
                  messageId: "personal-message",
                  inputId: "personal-input",
                  status: "queued",
                },
                startCompute: { intentId: "intent-1", status: "starting" },
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    return starts;
  }

  it("offers Start and send while stopped and sends startCompute with the message", async () => {
    serveVm("stopped");
    const starts = serveChatStart();
    renderRoute("/g5labs/me");
    const user = userEvent.setup();

    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toBeEnabled());
    expect(screen.getAllByTestId("start-and-send-note")[0]).toHaveTextContent(
      "Your personal workspace is stopped.",
    );
    await user.type(input, "summarize my notes");
    const send = screen.getByTestId("chat-panel-no-session-send");
    expect(send).toHaveTextContent("Start and send");
    await waitFor(() => expect(send).toBeEnabled());
    await user.click(send);

    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      content: "summarize my notes",
      startCompute: true,
    });
  });

  it("offers Start and send on an ordinary stopped workspace too and sends startCompute (g5.chat.start-and-send)", async () => {
    serveVm("stopped");
    const starts: Record<string, unknown>[] = [];
    const session = createMockChatSession({
      id: "ordinary-session",
      workspaceId: "task-graph-rework",
      scopeType: "workspace",
      scopeRef: "task-graph-rework",
    });
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({
          data: { orgId: "org-g5labs", workspaceId: "task-graph-rework" },
        }),
      ),
      http.get("/api/v1/workspaces/task-graph-rework", () =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails["task-graph-rework"],
            ownerUserId: "user-alex",
          },
        }),
      ),
      http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
        HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } }),
      ),
      http.get("/api/v1/workspaces/:id/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
      http.post(
        "/api/v1/workspaces/:id/sessions/start",
        async ({ request }) => {
          starts.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json(
            {
              data: {
                session,
                taskId: "ordinary-task",
                assistantMessageId: "ordinary-message",
                message: {
                  messageId: "ordinary-message",
                  inputId: "ordinary-input",
                  status: "queued",
                },
                startCompute: { intentId: "intent-2", status: "starting" },
              },
            },
            { status: 201 },
          );
        },
      ),
    );
    renderRoute("/g5labs/task-graph-rework");
    const user = userEvent.setup();
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toBeEnabled());
    expect(screen.getAllByTestId("start-and-send-note")[0]).toHaveTextContent(
      "The workspace is stopped.",
    );
    expect(
      screen.queryByTestId("chat-composer-disabled-reason-trigger"),
    ).toBeNull();
    await user.type(input, "plan the retry change");
    const send = screen.getByTestId("chat-panel-no-session-send");
    expect(send).toHaveTextContent("Start and send");
    await waitFor(() => expect(send).toBeEnabled());
    await user.click(send);
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({
      content: "plan the retry change",
      startCompute: true,
    });
  });

  it("keeps a reader's composer closed on a stopped workspace", async () => {
    serveVm("stopped");
    server.use(
      http.get("/api/v1/resolve", () =>
        HttpResponse.json({
          data: { orgId: "org-g5labs", workspaceId: "task-graph-rework" },
        }),
      ),
      http.get("/api/v1/workspaces/task-graph-rework", () =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails["task-graph-rework"],
            ownerUserId: "user-someone-else",
          },
        }),
      ),
    );
    renderRoute("/g5labs/task-graph-rework");
    const input = await screen.findByTestId("chat-input");
    await waitFor(() => expect(input).toBeDisabled());
    expect(screen.queryByText("Start and send")).toBeNull();
    expect(screen.queryByTestId("start-and-send-note")).toBeNull();
  });
});
