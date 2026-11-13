import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation } from "react-router-dom";
import { expect, it, vi } from "vitest";
import type { AgentLaunchPreferencesV1 } from "../lib/types";
import { GETTING_STARTED_AUTO_OPENED_STORAGE_KEY } from "../lib/userGuide";
import {
  createMockChatSession,
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

function Page() {
  const location = useLocation();
  return (
    <>
      <WorkspacePage />
      <div data-testid="route-probe">{location.search}</div>
    </>
  );
}

// Create PR and Merge both reach the Prepare-to-Ship surface from the ribbon
// (g5.workbench.ship-actions).
it.each(["manual", "automatic"] as const)(
  "retains the Prepare-to-Ship draft through destination refresh failure and %s retry",
  async (recovery) => {
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
    let available = true;
    const targets = [
      { id: "base", name: "Base" },
      { id: "alternate", name: "Alternate" },
    ];
    server.use(
      http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
        HttpResponse.json(
          { error: { code: "SERVICE_UNAVAILABLE", message: "Not configured" } },
          { status: 503 },
        ),
      ),
      http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/workspaces/:id/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...mockInstanceInfo,
            features: mockInstanceInfo.features,
          },
        }),
      ),
      http.get("/api/v1/workspaces/:id", ({ params }) =>
        !available && recovery === "automatic"
          ? HttpResponse.text("<html>failed_to_pick_backend</html>", {
              status: 502,
            })
          : HttpResponse.json({
              data: {
                ...mockWorkspaceDetails[String(params.id)],
                phase: "ie",
                ownerUserId: "user-alex",
                readOnly: false,
              },
            }),
      ),
      http.post("/api/v1/workspaces/:id/pull-requests/preview", () =>
        HttpResponse.json({
          data: {
            title: "Feature",
            body: "Feature changes",
            source: "commits",
          },
        }),
      ),
      http.get("/api/v1/workspaces/:id/pull-requests/targets", () =>
        available
          ? HttpResponse.json({ data: targets })
          : HttpResponse.json(
              {
                error: {
                  code: "SERVICE_UNAVAILABLE",
                  message: "Destination service unavailable",
                },
              },
              { status: 503 },
            ),
      ),
    );
    const { queryClient } = renderApp(
      <Routes>
        <Route path="/:org/:workspace" element={<Page />} />
      </Routes>,
      { route: "/org-g5labs/task-graph-rework", user: createMockUser() },
    );
    const open = await screen.findByTestId("ribbon-action-merge");
    await waitFor(() => expect(open).toBeEnabled());
    fireEvent.click(open);
    const modal = await screen.findByTestId("prepare-to-ship-modal");
    fireEvent.click(
      within(modal).getByTestId("prepare-to-ship-description-toggle"),
    );
    fireEvent.change(within(modal).getByLabelText("PR description"), {
      target: { value: "Keep my draft" },
    });
    const key = ["workspaces", "pull-request-targets", "task-graph-rework"];
    available = false;
    await act(async () => {
      await queryClient.invalidateQueries({ queryKey: key });
      if (recovery === "automatic") {
        await queryClient.invalidateQueries({
          queryKey: ["workspace", "task-graph-rework"],
          exact: true,
        });
      }
    });
    await screen.findByText(/Destination service unavailable/);
    expect(modal).toBeInTheDocument();
    const control = within(modal).getByLabelText("PR description");
    expect(control).toHaveValue("Keep my draft");
    expect(
      within(modal)
        .getByRole("button", { name: "Retry PR destinations" })
        .closest('[role="alert"]'),
    ).toHaveTextContent("Showing previously loaded destinations");
    if (recovery === "automatic") {
      expect(
        screen.getByText(/Can't refresh this workspace/),
      ).toBeInTheDocument();
    }
    available = true;
    if (recovery === "automatic") {
      // Only the detail read recovers: no target invalidation, focus or WS event.
      await act(async () => {
        await queryClient.refetchQueries({
          queryKey: ["workspace", "task-graph-rework"],
          exact: true,
        });
      });
      await waitFor(() =>
        expect(
          screen.queryByText(/Can't refresh this workspace/),
        ).not.toBeInTheDocument(),
      );
    } else {
      fireEvent.click(
        within(modal).getByRole("button", { name: "Retry PR destinations" }),
      );
    }
    await waitFor(() =>
      expect(
        within(modal).queryByRole("button", { name: "Retry PR destinations" }),
      ).not.toBeInTheDocument(),
    );
    expect(control).toHaveValue("Keep my draft");
  },
);

it("Get latest from Base submits the chosen agent and opens its resolution chat", async () => {
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
  const session = createMockChatSession({
    id: "resolution-session",
    workspaceId: "task-graph-rework",
    title: "Resolve Git conflicts",
    scopeType: "workspace",
    scopeRef: null,
  });
  let selection: AgentLaunchPreferencesV1 | undefined;
  const starts: unknown[] = [];
  server.use(
    http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.post("/api/v1/workspaces/:id/pull-requests/preview", () =>
      HttpResponse.json({
        data: { title: "Feature", body: "Feature changes", source: "commits" },
      }),
    ),
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...mockInstanceInfo,
          features: mockInstanceInfo.features,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id", ({ params }) =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails[String(params.id)],
          phase: "ie",
          ownerUserId: "user-alex",
          readOnly: false,
        },
      }),
    ),
    http.post(
      "/api/v1/workspaces/:id/config/agent-launch/resolve",
      async ({ request }) => {
        const { preferences } = (await request.json()) as {
          preferences: AgentLaunchPreferencesV1;
        };
        selection = preferences;
        return HttpResponse.json({
          data: {
            resolved: {
              schemaVersion: 1,
              resolutionId: "base-pull-confirmed",
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
        });
      },
    ),
    http.post("/api/v1/workspaces/:id/rebase-merge/preflight", () =>
      HttpResponse.json({
        data: {
          preflight: {
            sourceKind: "workspace",
            sourceWorkspaceId: "task-graph-rework",
            targetWorkspaceId: "base",
            decision: "gitConflicts",
            canDirectMerge: false,
            preflightFingerprint: "conflict",
            repoResults: [],
            message: "Conflicts",
          },
        },
      }),
    ),
    http.post("/api/v1/workspaces/:id/rebase-merge", async ({ request }) => {
      starts.push(await request.json());
      return HttpResponse.json({
        data: {
          resolution: {
            taskId: "resolution-task",
            sessionId: session.id,
            status: "queued",
          },
        },
      });
    }),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/resolution-session", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/resolution-session/task", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/sessions/resolution-session/messages", () =>
      HttpResponse.json({ data: [] }),
    ),
  );
  renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<Page />} />
    </Routes>,
    {
      route: "/org-g5labs/task-graph-rework",
      user: createMockUser(),
    },
  );
  const open = await screen.findByTestId("ribbon-action-merge");
  fireEvent.click(open);
  const modal = await screen.findByTestId("prepare-to-ship-modal");
  const update = within(modal).getByTestId("prepare-to-ship-update-codebase");
  await waitFor(() => expect(update).toBeEnabled());
  fireEvent.click(update);
  const resolve = await screen.findByTestId("direct-clean-merge-full-workflow");
  fireEvent.click(resolve);
  await waitFor(() =>
    expect(starts).toEqual([
      {
        targetWorkspaceId: "base",
        agentLaunch: {
          preferences: selection,
          confirmedResolutionId: "base-pull-confirmed",
        },
      },
    ]),
  );
  await waitFor(() =>
    expect(
      screen.queryByTestId("prepare-to-ship-modal"),
    ).not.toBeInTheDocument(),
  );
  // The resolution conversation opens in Chat and the URL keeps its hint
  // even though Chat takes the center from the timeline's `?view=activity`.
  await waitFor(() =>
    expect(screen.getByTestId("route-probe")).toHaveTextContent(
      "session=resolution-session",
    ),
  );
  const rightPanel = await screen.findByTestId("right-panel");
  expect(rightPanel).toBeVisible();
  expect(
    await within(rightPanel).findByText(String(session.title)),
  ).toBeInTheDocument();
  expect(
    screen.queryByText("Could not start workspace merge"),
  ).not.toBeInTheDocument();
});

// Start and resolve with agent (g5.git.stopped-rebase.resolve-with-agent,
// g5.chat.start-and-send): on the owner's stopped VM the conflicts dialog's
// agent choice is the compound Start, the request carries `startCompute`,
// the VM state is re-read and the chat opens on the queued brief.
it("Get latest from Base on a stopped VM starts it and queues the resolution brief", async () => {
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
  const session = createMockChatSession({
    id: "resolution-session",
    workspaceId: "task-graph-rework",
    title: "Resolve Git conflicts",
    scopeType: "workspace",
    scopeRef: null,
  });
  const starts: unknown[] = [];
  let vmReads = 0;
  let vmStatus: "stopped" | "starting" = "stopped";
  server.use(
    http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.post("/api/v1/workspaces/:id/pull-requests/preview", () =>
      HttpResponse.json({
        data: { title: "Feature", body: "Feature changes", source: "commits" },
      }),
    ),
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: { ...mockInstanceInfo, features: mockInstanceInfo.features },
      }),
    ),
    http.get("/api/v1/workspaces/:id", ({ params }) =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails[String(params.id)],
          phase: "ie",
          ownerUserId: "user-alex",
          readOnly: false,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id/vm", ({ params }) => {
      vmReads += 1;
      return HttpResponse.json({
        data: {
          orgId: "org-g5labs",
          workspaceId: String(params.id),
          ownerUserId: "user-alex",
          status: vmStatus,
          hostId: null,
          vmId: null,
          epoch: 1,
          imageRef: "g5-test:latest",
          moduleCatalogVersion: "test",
          startIntent: null,
          startDeadlineAt: null,
          startedAt: null,
          lastActivityAt: null,
          stopReason: null,
          stopOperationId: null,
          stopDeadlineAt: null,
          pendingStopReason: null,
          pendingStopDeadlineAt: null,
          nudgedAt: null,
          lastFlushId: null,
          unflushedLossAt: null,
          lastError: null,
        },
      });
    }),
    http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
      HttpResponse.json({
        data: {
          resolved: {
            schemaVersion: 1,
            resolutionId: "base-pull-confirmed",
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
    http.post("/api/v1/workspaces/:id/rebase-merge/preflight", () =>
      HttpResponse.json({
        data: {
          preflight: {
            sourceKind: "workspace",
            sourceWorkspaceId: "task-graph-rework",
            targetWorkspaceId: "base",
            decision: "gitConflicts",
            canDirectMerge: false,
            preflightFingerprint: "conflict",
            repoResults: [],
            message: "Conflicts",
          },
        },
      }),
    ),
    http.post("/api/v1/workspaces/:id/rebase-merge", async ({ request }) => {
      starts.push(await request.json());
      vmStatus = "starting";
      return HttpResponse.json({
        data: {
          resolution: {
            taskId: "resolution-task",
            sessionId: session.id,
            status: "queued",
          },
          startCompute: { intentId: "intent-1", status: "starting" },
        },
      });
    }),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/resolution-session", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/resolution-session/task", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/sessions/resolution-session/messages", () =>
      HttpResponse.json({ data: [] }),
    ),
  );
  renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<Page />} />
    </Routes>,
    { route: "/org-g5labs/task-graph-rework", user: createMockUser() },
  );
  fireEvent.click(await screen.findByTestId("ribbon-action-merge"));
  const modal = await screen.findByTestId("prepare-to-ship-modal");
  const update = within(modal).getByTestId("prepare-to-ship-update-codebase");
  await waitFor(() => expect(update).toBeEnabled());
  fireEvent.click(update);
  const resolve = await screen.findByTestId("direct-clean-merge-full-workflow");
  expect(resolve).toHaveTextContent("Start and resolve with agent");
  expect(
    screen.getByTestId("direct-clean-merge-start-and-resolve-note"),
  ).toHaveTextContent("The workspace is stopped.");
  const readsBefore = vmReads;
  fireEvent.click(resolve);
  await waitFor(() =>
    expect(starts).toEqual([
      expect.objectContaining({
        targetWorkspaceId: "base",
        startCompute: true,
      }),
    ]),
  );
  // The VM state is re-read after the Start was recorded: well inside the
  // stopped VM's five-second poll, and the page then shows the Start under
  // way (the chat's Start progress notice), which only a re-read can show.
  await waitFor(() => expect(vmReads).toBeGreaterThan(readsBefore), {
    timeout: 1500,
  });
  expect(await screen.findByTestId("start-progress")).toBeInTheDocument();
  await waitFor(() =>
    expect(
      screen.queryByTestId("prepare-to-ship-modal"),
    ).not.toBeInTheDocument(),
  );
  expect(
    await screen.findByText("Conflict resolution queued"),
  ).toBeInTheDocument();
  await waitFor(() =>
    expect(screen.getByTestId("route-probe")).toHaveTextContent(
      "session=resolution-session",
    ),
  );
});

// `g5.architecture.workspace_rebase`: a clean rebase the running VM
// completed is reported as done, not as a started workflow.
it("Get latest from Base reports a rebase the running workspace completed", async () => {
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
  const session = createMockChatSession({
    id: "resolution-session",
    workspaceId: "task-graph-rework",
    title: "Resolve Git conflicts",
    scopeType: "workspace",
    scopeRef: null,
  });
  let selection: AgentLaunchPreferencesV1 | undefined;
  const starts: unknown[] = [];
  server.use(
    http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
      HttpResponse.json({ data: [] }),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json({ data: null }),
    ),
    http.post("/api/v1/workspaces/:id/pull-requests/preview", () =>
      HttpResponse.json({
        data: { title: "Feature", body: "Feature changes", source: "commits" },
      }),
    ),
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({
        data: {
          ...mockInstanceInfo,
          features: mockInstanceInfo.features,
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id", ({ params }) =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails[String(params.id)],
          phase: "ie",
          ownerUserId: "user-alex",
          readOnly: false,
        },
      }),
    ),
    http.post(
      "/api/v1/workspaces/:id/config/agent-launch/resolve",
      async ({ request }) => {
        const { preferences } = (await request.json()) as {
          preferences: AgentLaunchPreferencesV1;
        };
        selection = preferences;
        return HttpResponse.json({
          data: {
            resolved: {
              schemaVersion: 1,
              resolutionId: "base-pull-confirmed",
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
        });
      },
    ),
    http.post("/api/v1/workspaces/:id/rebase-merge/preflight", () =>
      HttpResponse.json({
        data: {
          preflight: {
            sourceKind: "workspace",
            sourceWorkspaceId: "task-graph-rework",
            targetWorkspaceId: "base",
            decision: "cleanMergeNoGitConflicts",
            canDirectMerge: true,
            preflightFingerprint: "clean",
            repoResults: [],
            message: "Clean",
          },
        },
      }),
    ),
    http.post("/api/v1/workspaces/:id/rebase-merge", async ({ request }) => {
      starts.push(await request.json());
      return HttpResponse.json({
        data: {
          rebase: {
            sourceWorkspaceId: "task-graph-rework",
            targetWorkspaceId: "base",
            vmId: "vm-task-graph-rework",
            members: [
              {
                repoId: "repo-1",
                previousHead: "a".repeat(40),
                head: "b".repeat(40),
                published: true,
              },
            ],
          },
        },
      });
    }),
    http.get("/api/v1/workspaces/:id/sessions", () =>
      HttpResponse.json({ data: [session] }),
    ),
    http.get("/api/v1/sessions/resolution-session", () =>
      HttpResponse.json({ data: session }),
    ),
    http.get("/api/v1/sessions/resolution-session/task", () =>
      HttpResponse.json({ data: null }),
    ),
    http.get("/api/v1/sessions/resolution-session/messages", () =>
      HttpResponse.json({ data: [] }),
    ),
  );
  renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<Page />} />
    </Routes>,
    {
      route: "/org-g5labs/task-graph-rework",
      user: createMockUser(),
    },
  );
  const open = await screen.findByTestId("ribbon-action-merge");
  fireEvent.click(open);
  const modal = await screen.findByTestId("prepare-to-ship-modal");
  const update = within(modal).getByTestId("prepare-to-ship-update-codebase");
  await waitFor(() => expect(update).toBeEnabled());
  fireEvent.click(update);
  const approve = await screen.findByTestId("workspace-rebase-approve");
  fireEvent.click(approve);
  await waitFor(() => expect(starts).toHaveLength(1));
  expect(await screen.findByText("Workspace rebase completed")).toBeVisible();
  expect(
    screen.getByText(/rebased in its running VM and saved to G5/),
  ).toBeVisible();
  expect(
    screen.queryByText("Workspace rebase workflow started"),
  ).not.toBeInTheDocument();
});

it.each(["running", "stopped"] as const)(
  "shows PR destination failures and retries for a %s owner",
  async (status) => {
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
    let available = false;
    let requests = 0;
    let vmStarts = 0;
    server.use(
      http.post("/api/v1/workspaces/:id/vm/start", () => {
        vmStarts += 1;
        return HttpResponse.json({ data: null });
      }),
      http.get("/api/v1/orgs/:orgId/workspaces/open-question-metrics", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.get("/api/v1/workspaces/:id/provider-quota", () =>
        HttpResponse.json({ data: null }),
      ),
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...mockInstanceInfo,
            features: mockInstanceInfo.features,
          },
        }),
      ),
      // A stopped owner keeps durable authority: the VM endpoint below
      // models the stop, and the workspace stays read-write
      // (`g5.workspace.ui.gates`).
      http.get("/api/v1/workspaces/:id", ({ params }) =>
        HttpResponse.json({
          data: {
            ...mockWorkspaceDetails[String(params.id)],
            phase: "ie",
            ownerUserId: "user-alex",
          },
        }),
      ),
      http.get("/api/v1/workspaces/:id/vm", ({ params }) =>
        HttpResponse.json({
          data: {
            orgId: "org-g5labs",
            workspaceId: String(params.id),
            ownerUserId: "user-alex",
            status,
            hostId: status === "running" ? "host-test" : null,
            vmId: status === "running" ? `vm-${params.id}` : null,
            epoch: 1,
            imageRef: "g5-test:latest",
            moduleCatalogVersion: "test",
            startIntent: null,
            startDeadlineAt: null,
            startedAt: "2024-12-16T08:00:00Z",
            lastActivityAt: "2024-12-16T08:00:00Z",
            stopReason: null,
            stopOperationId: null,
            stopDeadlineAt: null,
            pendingStopReason: null,
            pendingStopDeadlineAt: null,
            nudgedAt: null,
            lastFlushId: null,
            unflushedLossAt: null,
            lastError: null,
          },
        }),
      ),
      http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
        HttpResponse.json(
          { error: { code: "SERVICE_UNAVAILABLE", message: "Not configured" } },
          { status: 503 },
        ),
      ),
      http.get("/api/v1/workspaces/:id/pull-requests/targets", () => {
        requests += 1;
        return available
          ? HttpResponse.json({ data: [{ id: "base", name: "Base" }] })
          : HttpResponse.json(
              {
                error: {
                  code: "SERVICE_UNAVAILABLE",
                  message: "Destination service unavailable",
                },
              },
              { status: 503 },
            );
      }),
    );
    renderApp(
      <Routes>
        <Route path="/:org/:workspace" element={<Page />} />
      </Routes>,
      {
        route: "/org-g5labs/task-graph-rework",
        user: createMockUser(),
      },
    );
    const retry = await screen.findByRole("button", {
      name: "Retry PR destinations",
    });
    expect(retry.closest('[role="alert"]')).toHaveTextContent(
      "Destination service unavailable",
    );
    // The ribbon keeps the action, disabled with the reason, until the
    // destinations are known (g5.ui.unavailable-actions).
    expect(screen.getByTestId("ribbon-action-merge")).toBeDisabled();
    const before = requests;
    available = true;
    fireEvent.click(retry);
    await waitFor(() => expect(requests).toBeGreaterThan(before));
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Retry PR destinations" }),
      ).not.toBeInTheDocument(),
    );
    if (status === "running") {
      expect(await screen.findByTestId("ribbon-action-merge")).toBeEnabled();
    }
    expect(screen.getByTestId("ribbon-action-create-pr")).toBeEnabled();
    expect(vmStarts).toBe(0);
    expect(
      screen.queryByRole("button", { name: "Retry PR destinations" }),
    ).not.toBeInTheDocument();
  },
);
