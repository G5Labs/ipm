import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { useState, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";

const panel = vi.hoisted(() => ({
  props: [] as Array<Record<string, unknown>>,
}));

// The run log's terminal view is a real xterm surface; here it is a stub
// that records what the dialog attaches it to.
vi.mock("../execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: (props: Record<string, unknown>) => {
    panel.props.push(props);
    return (
      <div
        data-testid="mock-terminal-panel"
        data-terminal-id={String(props.terminalId)}
        data-read-only={String(props.readOnly === true)}
      />
    );
  },
}));

import { PreviewControl } from "./PreviewControl";

const WORKSPACE_DEFAULT_APPLICATION = {
  workspaceId: "workspace-1",
  key: "branch-code:workspaceDefault:",
  branchId: "branch-code",
  repoId: "11111111-1111-4111-8111-111111111111",
  repoName: "code",
  branchName: "feature/preview",
  source: "workspaceDefault",
  profile: null,
  command: "npm run dev",
  port: 3000,
  description: null,
};

const state = vi.hoisted(() => ({
  ownerUserId: "user-owner",
  vmStatus: "running",
  applications: [] as Array<Record<string, unknown>>,
  /** The source the dialog last asked the discovery for. */
  applicationSource: null as string | null,
}));

vi.mock("../../hooks/useAuth", () => ({
  useAuth: () => ({ user: { id: "user-owner" } }),
}));

vi.mock("../../hooks/useWorkspace", () => ({
  useWorkspace: () => ({
    status: "ok",
    data: {
      ownerUserId: state.ownerUserId,
      docsRepoId: null,
      branches: [
        {
          workspaceId: "workspace-1",

          repoId: "11111111-1111-4111-8111-111111111111",
          repoName: "code",
          branchName: "feature/preview",
        },
      ],
    },
  }),
  useWorkspaceVm: () => ({
    status: "ok",
    data: { status: state.vmStatus },
  }),
}));

vi.mock("../../hooks/useTasks", async (importOriginal) => ({
  // Only the application discovery is scripted.
  ...(await importOriginal<typeof import("../../hooks/useTasks")>()),
  usePreviewApplications: (
    _branches: unknown,
    enabled: boolean,
    _mode?: unknown,
    source: string = "live",
  ) => {
    if (enabled) state.applicationSource = source;
    return enabled
      ? { status: "ok", data: state.applications }
      : { status: "loading" };
  },
}));

/** A page gate for a VM-state kind the owner sees (g5.ui.workspace-lifecycle-state). */
const ownerGate = (
  kind: "live" | "stopped" | "failed" | "starting" | "queued" | "stopping",
) => ({
  kind,
  canWrite: kind === "live",
  live: kind === "live",
  canStart: kind === "stopped" || kind === "failed",
  reviewCopy: false,
  reviewIdentityKnown: true,
  owner: true,
  workspaceNoun: "the workspace",
});

/** A preview task recorded with a Start and waiting for the VM. */
const PENDING_PREVIEW = {
  workspaceId: "workspace-1",
  taskId: "preview-task-pending",
  status: "queued",
  repoId: "11111111-1111-4111-8111-111111111111",
  profile: null,
  port: 3000,
  generation: 1,
  ready: false,
  closing: false,
  close: null,
  pendingStart: true,
};

function renderControl(props: Record<string, unknown> = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const Wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(
    <PreviewControl workspaceId="workspace-1" isDirty={false} {...props} />,
    { wrapper: Wrapper },
  );
}

describe("PreviewControl workspace service", () => {
  beforeEach(() => {
    state.ownerUserId = "user-owner";
    state.vmStatus = "running";
    state.applications = [WORKSPACE_DEFAULT_APPLICATION];
    state.applicationSource = null;
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({ data: null }),
      ),
    );
  });

  it("keeps Preview for a running read-only owner: the server checks live ownership, not policy (g5.workspace.ui.gates)", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <WorkspaceLiveGateProvider
          value={{
            kind: "read_only",
            canWrite: false,
            live: false,
            canStart: false,
            reviewCopy: false,
            reviewIdentityKnown: true,
            owner: true,
            workspaceNoun: "the workspace",
          }}
        >
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
    const trigger = await screen.findByTestId("preview-control-trigger");
    await waitFor(() => expect(trigger).toBeEnabled());
    expect(screen.queryByTestId("preview-control-trigger-hint")).toBeNull();
  });

  it("keeps Preview open for a read-only owner whose VM stopped: the VM state, not the policy, decides and it offers Start workspace and preview (g5.workspace.ui.gates)", async () => {
    state.vmStatus = "stopped";
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={client}>
        <WorkspaceLiveGateProvider
          value={{
            kind: "read_only",
            vmKind: "stopped",
            canWrite: false,
            live: false,
            canStart: false,
            reviewCopy: false,
            reviewIdentityKnown: true,
            owner: true,
            workspaceNoun: "the workspace",
          }}
        >
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
    const trigger = await screen.findByTestId("preview-control-trigger");
    expect(trigger).toBeEnabled();
    expect(trigger).toHaveAttribute(
      "title",
      "Start the workspace and launch a preview",
    );
    expect(screen.queryByTestId("preview-control-trigger-hint")).toBeNull();
    await user.click(trigger);
    expect(
      await screen.findByRole("button", {
        name: "Start workspace and preview",
      }),
    ).toBeEnabled();
  });

  it("offers Start workspace and preview while the owner's VM is stopped: the committed applications are listed and one request carries startCompute (g5.preview.start-and-preview)", async () => {
    state.vmStatus = "stopped";
    let body: Record<string, unknown> | null = null;
    server.use(
      http.post(
        "/api/v1/workspaces/workspace-1/preview",
        async ({ request }) => {
          body = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json({
            data: {
              ...PENDING_PREVIEW,
              startCompute: { intentId: "intent-1", status: "starting" },
            },
          });
        },
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const invalidated = vi.spyOn(client, "invalidateQueries");
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={client}>
        <WorkspaceLiveGateProvider value={ownerGate("stopped")}>
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
    const trigger = await screen.findByTestId("preview-control-trigger");
    expect(trigger).toBeEnabled();
    expect(screen.queryByTestId("preview-control-trigger-hint")).toBeNull();
    await user.click(trigger);
    expect(
      await screen.findByTestId("workspace-preview-committed-note"),
    ).toHaveTextContent("as committed on the branch");
    expect(state.applicationSource).toBe("committed");
    const start = screen.getByTestId("workspace-preview-start");
    expect(start).toHaveTextContent("Start workspace and preview");
    expect(start).toBeEnabled();
    expect(screen.queryByTestId("workspace-preview-start-hint")).toBeNull();

    await user.click(start);
    await waitFor(() => expect(body).not.toBeNull());
    expect(body).toMatchObject({
      repoId: "11111111-1111-4111-8111-111111111111",
      profile: null,
      startCompute: true,
    });
    // The recorded Start is reflected everywhere the VM state is read.
    expect(invalidated).toHaveBeenCalledWith({ queryKey: ["workspace-vm"] });
    expect(
      await screen.findByText(
        "Starting the workspace… the preview launches once it is running",
      ),
    ).toBeVisible();
    expect(screen.getByTestId("workspace-preview-stop")).toHaveAttribute(
      "title",
      "Cancel the preview; the workspace keeps starting",
    );
  });

  it("waits under a Start already under way: Preview once running sends no second Start, and names the capacity wait", async () => {
    state.vmStatus = "starting";
    let body: Record<string, unknown> | null = null;
    server.use(
      http.post(
        "/api/v1/workspaces/workspace-1/preview",
        async ({ request }) => {
          body = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json({ data: PENDING_PREVIEW });
        },
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const user = userEvent.setup();
    const { rerender } = render(
      <QueryClientProvider client={client}>
        <WorkspaceLiveGateProvider value={ownerGate("starting")}>
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
    const trigger = await screen.findByTestId("preview-control-trigger");
    expect(trigger).toHaveAttribute(
      "title",
      "Launch a preview once the workspace is running",
    );
    await user.click(trigger);
    const start = await screen.findByTestId("workspace-preview-start");
    expect(start).toHaveTextContent("Preview once running");
    await user.click(start);
    await waitFor(() => expect(body).not.toBeNull());
    expect(body).not.toHaveProperty("startCompute");
    expect(
      await screen.findByText(
        "Starting the workspace… the preview launches once it is running",
      ),
    ).toBeVisible();

    rerender(
      <QueryClientProvider client={client}>
        <WorkspaceLiveGateProvider value={ownerGate("queued")}>
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
    expect(
      await screen.findByText(
        "Waiting for capacity to start the workspace… the preview launches once it is running",
      ),
    ).toBeVisible();
  });

  it("shows a preview waiting for the Start after a reload: the trigger lights and the dialog reads the pending state", async () => {
    state.vmStatus = "starting";
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({ data: PENDING_PREVIEW }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    const trigger = screen.getByRole("button", { name: "Preview" });
    await waitFor(() =>
      expect(trigger).toHaveAttribute("data-preview-state", "starting"),
    );
    await user.click(trigger);
    expect(
      await screen.findByText(
        "Starting the workspace… the preview launches once it is running",
      ),
    ).toBeVisible();
    expect(screen.getByTestId("workspace-preview-stop")).toBeEnabled();
    expect(screen.queryByTestId("workspace-preview-start")).toBeNull();
  });

  it("explains a preview whose Start ended before it ran and offers Start workspace and preview again", async () => {
    state.vmStatus = "stopped";
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            ...PENDING_PREVIEW,
            status: "cancelled",
            pendingStart: false,
            ending: {
              kind: "did_not_start",
              reason: "start_ended",
              message: "the workspace Start ended before the preview ran",
            },
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    expect(
      await screen.findByTestId("workspace-preview-last-failed"),
    ).toHaveTextContent(
      "Could not be started: the workspace Start ended before the preview ran.",
    );
    expect(screen.queryByTestId("workspace-preview-log-toggle")).toBeNull();
    expect(
      screen.getByRole("button", { name: "Start workspace and preview" }),
    ).toBeEnabled();
  });

  it("keeps a Base workspace's Preview behind its reason: only a VM state opens the launcher", async () => {
    state.vmStatus = "stopped";
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <WorkspaceLiveGateProvider
          value={{
            kind: "base",
            canWrite: false,
            live: false,
            canStart: false,
            reviewCopy: false,
            reviewIdentityKnown: true,
            owner: true,
            workspaceNoun: "the workspace",
          }}
        >
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      </QueryClientProvider>,
    );
    const trigger = await screen.findByTestId("preview-control-trigger");
    expect(trigger).toBeDisabled();
    expect(trigger).toHaveAttribute(
      "title",
      "Read-only workspace. Fork workspace to make edits.",
    );
  });

  it("renders no merged-state preview control while the VM is stopped (merged previews keep the running-VM rule)", () => {
    state.vmStatus = "stopped";
    renderControl({
      mergeResult: { preflightFingerprint: "fp-1" },
    });
    expect(
      screen.queryByRole("button", { name: "Preview" }),
    ).not.toBeInTheDocument();
  });

  it("creates a typed preview task for the selected repository profile", async () => {
    let body: Record<string, unknown> | null = null;
    server.use(
      http.post(
        "/api/v1/workspaces/workspace-1/preview",
        async ({ request }) => {
          body = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json({
            data: {
              workspaceId: "workspace-1",
              taskId: "preview-task-1",
              status: "queued",
              repoId: "11111111-1111-4111-8111-111111111111",
              profile: null,
              port: 3000,
              generation: 1,
              ready: false,
            },
          });
        },
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await user.click(screen.getByRole("button", { name: "Start preview" }));

    await waitFor(() => expect(body).not.toBeNull());
    const requestBody = body as Record<string, unknown> | null;
    expect(body).toMatchObject({
      repoId: "11111111-1111-4111-8111-111111111111",
      profile: null,
    });
    expect(requestBody?.idempotencyKey).toEqual(expect.any(String));
    expect(
      await screen.findByText("Starting the configured application…"),
    ).toBeVisible();
  });

  it("turns Start preview into Start workspace and preview in the open dialog once the workspace stops (g5.preview.start-and-preview)", async () => {
    let body: Record<string, unknown> | null = null;
    server.use(
      http.post(
        "/api/v1/workspaces/workspace-1/preview",
        async ({ request }) => {
          body = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json({ data: PENDING_PREVIEW });
        },
      ),
    );
    const gate = (live: boolean) => ({
      kind: live ? ("live" as const) : ("stopped" as const),
      canWrite: true,
      live,
      canStart: !live,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: true,
      workspaceNoun: "the workspace",
    });
    function Host() {
      const [live, setLive] = useState(true);
      return (
        <WorkspaceLiveGateProvider value={gate(live)}>
          <button
            type="button"
            data-testid="stop-workspace"
            onClick={() => {
              state.vmStatus = "stopped";
              setLive(false);
            }}
          >
            stop
          </button>
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      );
    }
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={client}>
        <Host />
      </QueryClientProvider>,
    );
    await user.click(await screen.findByRole("button", { name: "Preview" }));
    const start = await screen.findByTestId("workspace-preview-start");
    expect(start).toBeEnabled();
    expect(start).toHaveTextContent("Start preview");
    expect(state.applicationSource).toBe("live");
    expect(screen.queryByTestId("workspace-preview-start-hint")).toBeNull();

    await user.click(screen.getByTestId("stop-workspace"));

    await waitFor(() =>
      expect(start).toHaveTextContent("Start workspace and preview"),
    );
    expect(start).toBeEnabled();
    expect(state.applicationSource).toBe("committed");
    expect(screen.queryByTestId("workspace-preview-start-hint")).toBeNull();
    await user.click(start);
    await waitFor(() => expect(body).not.toBeNull());
    expect(body).toMatchObject({ startCompute: true });
  });

  it("offers a read-only owner Start workspace and preview in the open dialog once the VM stops (g5.workspace.ui.gates)", async () => {
    let body: Record<string, unknown> | null = null;
    server.use(
      http.post(
        "/api/v1/workspaces/workspace-1/preview",
        async ({ request }) => {
          body = (await request.json()) as Record<string, unknown>;
          return HttpResponse.json({ data: PENDING_PREVIEW });
        },
      ),
    );
    const gate = (live: boolean) => ({
      kind: "read_only" as const,
      vmKind: live ? ("live" as const) : ("stopped" as const),
      canWrite: false,
      live: false,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner: true,
      workspaceNoun: "the workspace",
    });
    function Host() {
      const [live, setLive] = useState(true);
      return (
        <WorkspaceLiveGateProvider value={gate(live)}>
          <button
            type="button"
            data-testid="stop-workspace"
            onClick={() => {
              state.vmStatus = "stopped";
              setLive(false);
            }}
          >
            stop
          </button>
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </WorkspaceLiveGateProvider>
      );
    }
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={client}>
        <Host />
      </QueryClientProvider>,
    );
    await user.click(await screen.findByRole("button", { name: "Preview" }));
    const start = await screen.findByTestId("workspace-preview-start");
    expect(start).toBeEnabled();

    await user.click(screen.getByTestId("stop-workspace"));

    await waitFor(() =>
      expect(start).toHaveTextContent("Start workspace and preview"),
    );
    expect(start).toBeEnabled();
    expect(screen.queryByTestId("workspace-preview-start-hint")).toBeNull();
    await user.click(start);
    await waitFor(() => expect(body).not.toBeNull());
    expect(body).toMatchObject({ startCompute: true });
  });

  it("opens and closes the current ready preview by task id", async () => {
    let cancelled = false;
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "running",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: null,
            port: 3000,
            generation: 1,
            ready: true,
            closing: false,
            close: null,
          },
        }),
      ),
      http.post("/api/v1/preview/session/preview-task-1", () =>
        HttpResponse.json({
          data: { previewUrl: "/api/v1/preview/preview-task-1/" },
        }),
      ),
      http.post("/api/v1/workspaces/workspace-1/preview/cancel", () => {
        cancelled = true;
        return HttpResponse.json({
          data: { taskId: "preview-task-1", status: "cancelling" },
        });
      }),
    );
    const replace = vi.fn();
    vi.spyOn(window, "open").mockReturnValue({
      opener: null,
      location: { replace },
      close: vi.fn(),
    } as unknown as Window);
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await user.click(
      await screen.findByRole("button", { name: "Open preview" }),
    );
    await waitFor(() =>
      expect(replace).toHaveBeenCalledWith("/api/v1/preview/preview-task-1/"),
    );
    expect(
      screen.queryByRole("button", { name: "Start preview" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Stop preview" }));
    await waitFor(() => expect(cancelled).toBe(true));
  });

  it("names the running profile and its description while the preview serves", async () => {
    state.applications = [
      WORKSPACE_DEFAULT_APPLICATION,
      {
        ...WORKSPACE_DEFAULT_APPLICATION,
        key: "branch-code:namedProfile:full-stack",
        source: "namedProfile",
        profile: "full-stack",
        command: "just dev-up",
        port: 13000,
        description: "Whole G5 stack with an attached worker host",
      },
    ];
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "running",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: "full-stack",
            port: 13000,
            generation: 1,
            ready: true,
            closing: false,
            close: null,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    expect(
      await screen.findByTestId("workspace-preview-profile"),
    ).toHaveTextContent(
      "full-stack · Whole G5 stack with an attached worker host",
    );
    expect(screen.getByTestId("workspace-preview-state")).toHaveTextContent(
      "Serving on port 13000",
    );
  });

  it("names a running workspace-default preview without inventing a description", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "running",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: null,
            port: 3000,
            generation: 1,
            ready: false,
            closing: false,
            close: null,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    const profile = await screen.findByTestId("workspace-preview-profile");
    expect(profile).toHaveTextContent("Workspace default");
    expect(profile).not.toHaveTextContent("·");
  });

  it("shows closing in progress and offers nothing else until the guest reports", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "running",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: "full-stack",
            port: 3000,
            generation: 1,
            ready: true,
            closing: true,
            close: null,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    expect(
      await screen.findByTestId("workspace-preview-state"),
    ).toHaveTextContent("Stopping the application");
    expect(screen.getByRole("button", { name: "Stopping…" })).toBeDisabled();
    // Open keeps its own name while a stop runs: the pending label belongs
    // to Stop alone.
    expect(screen.getByRole("button", { name: "Open preview" })).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "Start preview" }),
    ).not.toBeInTheDocument();
  });

  it("words a lost run's notice as the Activity row does and offers no log for it", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "failed",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: "full-stack",
            port: 3000,
            generation: 1,
            ready: true,
            closing: false,
            close: null,
            ending: { kind: "failed", cause: "vm_lost", reason: "idle_stop" },
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Preview/ })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: /Preview/ }));
    const notice = await screen.findByTestId("workspace-preview-last-failed");
    expect(notice).toHaveTextContent(
      "Lost with the workspace VM while it was being stopped because the workspace went idle.",
    );
    expect(notice).not.toHaveTextContent("exited with an error");
    expect(notice.querySelector("button")).toBeNull();
  });

  it("answers a row's request against a fresh read when the cached preview lags behind Activity", async () => {
    panel.props.length = 0;
    let reads = 0;
    const run = (taskId: string) => ({
      workspaceId: "workspace-1",
      taskId,
      status: "running",
      repoId: "11111111-1111-4111-8111-111111111111",
      profile: "full-stack",
      port: 3000,
      generation: 1,
      ready: true,
      closing: false,
      close: null,
    });
    server.use(
      // The first read still names run A; every later read names run B,
      // the run the Activity row describes.
      http.get("/api/v1/workspaces/workspace-1/preview", () => {
        reads += 1;
        return HttpResponse.json({
          data: reads === 1 ? run("preview-task-1") : run("preview-task-2"),
        });
      }),
      http.get("/api/v1/tasks/preview-task-2/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e1", "invocation.started", {}),
              timelineEntry("e2", "module.preview.ready", { port: 3000 }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const { rerender } = renderControl();
    await waitFor(() => expect(reads).toBe(1));
    rerender(
      <PreviewControl
        workspaceId="workspace-1"
        isDirty={false}
        openRequest={{ taskId: "preview-task-2", log: true, seq: 1 }}
      />,
    );
    await screen.findByTestId("workspace-preview-log-toggle");
    const terminal = await screen.findByTestId(
      "mock-terminal-panel",
      {},
      { timeout: 4000 },
    );
    expect(terminal).toHaveAttribute(
      "data-terminal-id",
      "preview:preview-task-2",
    );
    expect(screen.getByTestId("workspace-preview-log-toggle")).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("treats a queued run cancelled by a lost VM as a failed last run", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "cancelled",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: "full-stack",
            port: 3000,
            generation: 1,
            ready: false,
            closing: false,
            close: null,
            ending: { kind: "failed", cause: "vm_lost", reason: "vm_lost" },
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Preview/ })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: /Preview/ }));
    const notice = await screen.findByTestId("workspace-preview-last-failed");
    expect(notice).toHaveTextContent("Lost with the workspace VM.");
    expect(notice.querySelector("button")).toBeNull();
    expect(
      screen.queryByTestId("workspace-preview-last-close-failed"),
    ).toBeNull();
  });

  it("tells a close cut short by a lost VM apart: no leftover state, no log (g5.ui.activity.preview-row)", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "cancelled",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: "full-stack",
            port: 3000,
            generation: 1,
            ready: true,
            closing: false,
            close: "lost",
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Preview/ })).toHaveAttribute(
        "data-preview-state",
        "failed",
      ),
    );
    await user.click(screen.getByRole("button", { name: /Preview/ }));
    const notice = await screen.findByTestId(
      "workspace-preview-last-close-lost",
    );
    expect(notice).toHaveTextContent(
      "The last preview (full-stack) was closed, but the workspace VM was lost before its cleanup finished.",
    );
    expect(notice).not.toHaveTextContent("leftover");
    expect(notice.querySelector("button")).toBeNull();
    expect(
      screen.queryByTestId("workspace-preview-last-close-failed"),
    ).toBeNull();
    expect(
      await screen.findByRole("button", { name: "Start preview" }),
    ).toBeInTheDocument();
  });

  it("reports a close whose cleanup failed and still offers a fresh start", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "cancelled",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: "full-stack",
            port: 3000,
            generation: 1,
            ready: true,
            closing: false,
            close: "failed",
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    const notice = await screen.findByTestId(
      "workspace-preview-last-close-failed",
    );
    expect(notice).toHaveTextContent(
      "The last preview (full-stack) closed, but its cleanup command failed.",
    );
    expect(
      await screen.findByRole("button", { name: "Start preview" }),
    ).toBeInTheDocument();
  });

  it("stays quiet about a clean close", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "cancelled",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: null,
            port: 3000,
            generation: 1,
            ready: true,
            closing: false,
            close: "clean",
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    expect(
      await screen.findByRole("button", { name: "Start preview" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("workspace-preview-last-close-failed"),
    ).not.toBeInTheDocument();
  });

  it("shows the conflict when a start is refused beside an active preview", async () => {
    server.use(
      http.post("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message:
                "a preview is already running; close it before starting another",
            },
          },
          { status: 409 },
        ),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await user.click(
      await screen.findByRole("button", { name: "Start preview" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "close it before starting another",
    );
  });

  it("explains a failed last attempt and still offers a fresh start", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "failed",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: "full-stack",
            port: 3000,
            generation: 1,
            ready: true,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));

    const notice = await screen.findByTestId("workspace-preview-last-failed");
    expect(notice).toHaveTextContent(
      "The last preview attempt (full-stack) on port 3000 failed.",
    );
    // A stale ready flag on a failed task must not resurface "Open preview".
    expect(
      screen.queryByRole("button", { name: "Open preview" }),
    ).not.toBeInTheDocument();
    expect(
      await screen.findByRole("button", { name: "Start preview" }),
    ).toBeInTheDocument();
  });

  it("keeps Open disabled until the current preview reports ready", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "running",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: null,
            port: 3000,
            generation: 1,
            ready: false,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));

    expect(
      await screen.findByText("Starting the configured application…"),
    ).toBeVisible();
    // Not ready: Open says so itself and is inert, while Stop stays an
    // available action (issue #786).
    const open = screen.getByTestId("workspace-preview-open");
    expect(open).toBeDisabled();
    expect(open).toHaveTextContent("Starting…");
    expect(open).toHaveAttribute("aria-busy", "true");
    expect(
      screen.queryByRole("button", { name: "Open preview" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop preview" })).toBeEnabled();
  });

  it("renders one dialog shell whose × dismisses without stopping the preview", async () => {
    let cancelled = false;
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "running",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: null,
            port: 3000,
            generation: 1,
            ready: true,
            closing: false,
            close: null,
          },
        }),
      ),
      http.post("/api/v1/workspaces/workspace-1/preview/cancel", () => {
        cancelled = true;
        return HttpResponse.json({ data: {} });
      }),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Workspace preview",
    });
    expect(dialog).toBe(screen.getByTestId("workspace-preview-dialog"));
    // The content renders straight into the shared Modal panel: no second
    // bordered card between the panel and the title.
    const title = screen.getByRole("heading", { name: "Workspace preview" });
    expect(title.parentElement?.parentElement).toBe(dialog);
    expect(screen.getByRole("button", { name: "Open preview" })).toBeEnabled();
    expect(screen.getByTestId("workspace-preview-state")).toHaveTextContent(
      "Serving on port 3000 · ready to open",
    );
    await user.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(cancelled).toBe(false);
  });

  it("shows an error and closes the tab when the session mint fails", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            workspaceId: "workspace-1",
            taskId: "preview-task-1",
            status: "running",
            repoId: "11111111-1111-4111-8111-111111111111",
            profile: null,
            port: 3000,
            generation: 1,
            ready: true,
            closing: false,
            close: null,
          },
        }),
      ),
      http.post("/api/v1/preview/session/preview-task-1", () =>
        HttpResponse.json(
          { error: { message: "Preview authorization unavailable" } },
          { status: 503 },
        ),
      ),
    );
    const close = vi.fn();
    vi.spyOn(window, "open").mockReturnValue({
      opener: null,
      location: { replace: vi.fn() },
      close,
    } as unknown as Window);
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await user.click(
      await screen.findByRole("button", { name: "Open preview" }),
    );

    await waitFor(() => expect(close).toHaveBeenCalled());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Preview authorization unavailable",
    );
  });

  const RUNNING_PREVIEW = {
    workspaceId: "workspace-1",
    taskId: "preview-task-1",
    status: "running",
    repoId: "11111111-1111-4111-8111-111111111111",
    profile: null,
    port: 3000,
    generation: 1,
    ready: true,
    closing: false,
    close: null,
  };

  it("lights the trigger while a preview serves without opening the dialog, and says so", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({ data: RUNNING_PREVIEW }),
      ),
    );
    renderControl();
    const trigger = screen.getByRole("button", { name: "Preview" });
    await waitFor(() =>
      expect(trigger).toHaveAttribute("data-preview-state", "serving"),
    );
    expect(trigger).toHaveAttribute("title", "Preview — serving on port 3000");
    expect(trigger).toHaveAccessibleDescription("serving on port 3000");
    expect(
      trigger.querySelector("svg[data-preview-state='serving']"),
    ).not.toBeNull();
  });

  it("keeps the trigger red after a failed run, and off while the VM is stopped", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: { ...RUNNING_PREVIEW, status: "failed", ready: false },
        }),
      ),
    );
    const { unmount } = renderControl();
    const trigger = screen.getByRole("button", { name: "Preview" });
    await waitFor(() =>
      expect(trigger).toHaveAttribute("data-preview-state", "failed"),
    );
    expect(trigger).toHaveAccessibleDescription(
      "the last run failed; open for its log",
    );
    unmount();
    state.vmStatus = "stopped";
    renderControl();
    expect(screen.getByRole("button", { name: "Preview" })).toHaveAttribute(
      "data-preview-state",
      "none",
    );
  });

  it("lights the trigger for a preview waiting on the owner's Start while the VM is still stopped", async () => {
    state.vmStatus = "stopped";
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({ data: PENDING_PREVIEW }),
      ),
    );
    renderControl();
    const trigger = screen.getByRole("button", { name: "Preview" });
    await waitFor(() =>
      expect(trigger).toHaveAttribute("data-preview-state", "starting"),
    );
    expect(trigger).toHaveAccessibleDescription("starting…");
  });

  const timelineEntry = (
    id: string,
    kind: string,
    payload: Record<string, unknown>,
  ) => ({
    id,
    entryType: "checkpoint",
    content: kind,
    metadata: { kind, payload },
    createdAt: "2026-10-20T10:42:01Z",
  });

  it("shows the run log on request: stages from the view, the application's output in a read-only terminal attached by task (g5.preview.run-log)", async () => {
    panel.props.length = 0;
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({ data: RUNNING_PREVIEW }),
      ),
      http.get("/api/v1/tasks/preview-task-1/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e1", "invocation.started", {}),
              timelineEntry("e2", "module.preview.ready", { port: 3000 }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    const toggle = await screen.findByTestId("workspace-preview-log-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByTestId("workspace-preview-run-log"),
    ).not.toBeInTheDocument();
    await user.click(toggle);
    await screen.findByTestId("workspace-preview-run-log");
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const terminal = screen.getByTestId("mock-terminal-panel");
    expect(terminal).toHaveAttribute(
      "data-terminal-id",
      "preview:preview-task-1",
    );
    expect(terminal).toHaveAttribute("data-read-only", "true");
    expect(panel.props[0]).toMatchObject({
      workspaceId: "workspace-1",
      enabled: true,
    });
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-preview-stage-serving"),
      ).toHaveAttribute("data-state", "current"),
    );
    expect(
      screen.getByTestId("workspace-preview-stage-building"),
    ).toHaveAttribute("data-state", "done");
    // Nothing of the output is read from the timeline: the entries are
    // lifecycle only and the log shows no line list.
    expect(screen.queryByTestId("workspace-preview-log-line")).toBeNull();
    await user.click(toggle);
    expect(
      screen.queryByTestId("workspace-preview-run-log"),
    ).not.toBeInTheDocument();
  });

  it("opens a failed run's log from its notice with the retained output, the exit status and stderr tail", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: { ...RUNNING_PREVIEW, status: "failed", ready: false },
        }),
      ),
      http.get("/api/v1/tasks/preview-task-1/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e1", "invocation.started", {}),
              timelineEntry("e3", "invocation.outcome", {
                status: "failed",
                exit_code: 1,
                result: {
                  status: "app_exited",
                  app_exit_code: 7,
                  app_signal: null,
                  stderr_tail: "Error: listen EADDRINUSE :3000\n",
                  stderr_tail_truncated: false,
                  message: "preview app exited with status Some(7)",
                },
              }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    const notice = await screen.findByTestId("workspace-preview-last-failed");
    await user.click(within(notice).getByRole("button", { name: "View log" }));
    const failure = await screen.findByTestId("workspace-preview-run-failure");
    expect(failure).toHaveTextContent("Application exited with status 7");
    expect(
      screen.getByTestId("workspace-preview-stderr-tail"),
    ).toHaveTextContent("Error: listen EADDRINUSE :3000");
    expect(
      screen.getByTestId("workspace-preview-stage-building"),
    ).toHaveAttribute("data-state", "failed");
    // While the VM runs the ended session's retained output is attached.
    expect(screen.getByTestId("mock-terminal-panel")).toHaveAttribute(
      "data-terminal-id",
      "preview:preview-task-1",
    );
    // The launcher stays available under the log.
    expect(
      screen.getByRole("button", { name: "Start preview" }),
    ).toBeInTheDocument();
    expect(
      within(notice).getByRole("button", { name: "Hide log" }),
    ).toBeInTheDocument();
  });

  it("drops the terminal once the VM stops while a failed run's log is open: stages, the notice and the tail remain", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: { ...RUNNING_PREVIEW, status: "failed", ready: false },
        }),
      ),
      http.get("/api/v1/tasks/preview-task-1/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e3", "invocation.outcome", {
                status: "failed",
                result: {
                  status: "app_exited",
                  app_exit_code: 2,
                  stderr_tail: "npm ERR! missing script: dev\n",
                  stderr_tail_truncated: true,
                },
              }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    function Host() {
      const [, rerender] = useState(0);
      return (
        <>
          <button
            type="button"
            data-testid="stop-workspace"
            onClick={() => {
              state.vmStatus = "stopped";
              rerender((n) => n + 1);
            }}
          >
            stop
          </button>
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </>
      );
    }
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={client}>
        <Host />
      </QueryClientProvider>,
    );
    await user.click(await screen.findByRole("button", { name: "Preview" }));
    const notice = await screen.findByTestId("workspace-preview-last-failed");
    await user.click(within(notice).getByRole("button", { name: "View log" }));
    await screen.findByTestId("workspace-preview-run-failure");
    expect(screen.getByTestId("mock-terminal-panel")).toBeInTheDocument();

    await user.click(screen.getByTestId("stop-workspace"));
    await waitFor(() =>
      expect(screen.queryByTestId("mock-terminal-panel")).toBeNull(),
    );
    expect(
      screen.getByTestId("workspace-preview-output-unavailable"),
    ).toHaveTextContent("Output is not retained after the VM stops.");
    expect(
      screen.getByTestId("workspace-preview-stderr-tail"),
    ).toHaveTextContent("stderr (last 4 KiB):");
    expect(
      screen.getByTestId("workspace-preview-stage-closed"),
    ).toHaveTextContent("Failed");
  });

  it("widens the shell only while a log is rendered: narrow again after a clean stop, with Log collapsed for the next run (g5.preview.dialog-controls)", async () => {
    let preview: Record<string, unknown> = RUNNING_PREVIEW;
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({ data: preview }),
      ),
      http.post("/api/v1/workspaces/workspace-1/preview", () => {
        preview = RUNNING_PREVIEW;
        return HttpResponse.json({ data: RUNNING_PREVIEW });
      }),
      http.post("/api/v1/workspaces/workspace-1/preview/cancel", () => {
        // The guest stops the application and cleans up; the view settles.
        preview = {
          ...RUNNING_PREVIEW,
          status: "cancelled",
          ready: false,
          closing: false,
          close: "clean",
        };
        return HttpResponse.json({ data: { ok: true } });
      }),
      http.get("/api/v1/tasks/preview-task-1/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [timelineEntry("e1", "invocation.started", {})],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    const dialog = await screen.findByTestId("workspace-preview-dialog");
    expect(dialog).toHaveClass("max-w-md");
    await user.click(await screen.findByTestId("workspace-preview-log-toggle"));
    await screen.findByTestId("workspace-preview-run-log");
    expect(dialog).toHaveClass("max-w-5xl");

    await user.click(screen.getByRole("button", { name: "Stop preview" }));
    // A clean stop: the launcher, no log, no notice, and the width the
    // dialog had before the log was opened.
    await screen.findByRole("button", { name: "Start preview" });
    expect(screen.queryByTestId("workspace-preview-run-log")).toBeNull();
    expect(screen.queryByTestId("workspace-preview-log-toggle")).toBeNull();
    expect(screen.getByTestId("workspace-preview-dialog")).toHaveClass(
      "max-w-md",
    );

    // The next run does not inherit the toggle: Log is collapsed.
    await user.click(screen.getByRole("button", { name: "Start preview" }));
    const toggle = await screen.findByTestId("workspace-preview-log-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("workspace-preview-run-log")).toBeNull();
    expect(screen.getByTestId("workspace-preview-dialog")).toHaveClass(
      "max-w-md",
    );
  });

  it("keeps the wide shell for a failed run's log, VM running or stopped", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: { ...RUNNING_PREVIEW, status: "failed", ready: false },
        }),
      ),
      http.get("/api/v1/tasks/preview-task-1/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e3", "invocation.outcome", {
                status: "failed",
                result: { status: "app_exited", app_exit_code: 2 },
              }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    function Host() {
      const [, rerender] = useState(0);
      return (
        <>
          <button
            type="button"
            data-testid="stop-workspace"
            onClick={() => {
              state.vmStatus = "stopped";
              rerender((n) => n + 1);
            }}
          >
            stop
          </button>
          <PreviewControl workspaceId="workspace-1" isDirty={false} />
        </>
      );
    }
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={client}>
        <Host />
      </QueryClientProvider>,
    );
    await user.click(await screen.findByRole("button", { name: "Preview" }));
    const dialog = await screen.findByTestId("workspace-preview-dialog");
    expect(dialog).toHaveClass("max-w-md");
    const notice = await screen.findByTestId("workspace-preview-last-failed");
    await user.click(within(notice).getByRole("button", { name: "View log" }));
    await screen.findByTestId("workspace-preview-run-failure");
    expect(dialog).toHaveClass("max-w-5xl");
    // The VM stops: the log loses its terminal but stays rendered with the
    // notice, so the shell stays wide.
    await user.click(screen.getByTestId("stop-workspace"));
    await waitFor(() =>
      expect(screen.queryByTestId("mock-terminal-panel")).toBeNull(),
    );
    expect(screen.getByTestId("workspace-preview-run-log")).toBeInTheDocument();
    expect(screen.getByTestId("workspace-preview-dialog")).toHaveClass(
      "max-w-5xl",
    );
    await user.click(within(notice).getByRole("button", { name: "Hide log" }));
    expect(screen.getByTestId("workspace-preview-dialog")).toHaveClass(
      "max-w-md",
    );
  });

  it("stays quiet about a clean close: no log link for the last run (owner decision)", async () => {
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({
          data: {
            ...RUNNING_PREVIEW,
            status: "cancelled",
            ready: false,
            close: "clean",
          },
        }),
      ),
    );
    const user = userEvent.setup();
    renderControl();
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await screen.findByRole("button", { name: "Start preview" });
    expect(screen.queryByTestId("workspace-preview-log-toggle")).toBeNull();
    expect(screen.queryByTestId("workspace-preview-run-log")).toBeNull();
  });

  it("does not expose preview controls to a reader", () => {
    state.ownerUserId = "someone-else";
    renderControl();
    expect(
      screen.queryByRole("button", { name: "Preview" }),
    ).not.toBeInTheDocument();
  });

  it("offers the explicit Start with the preview instead of requiring it first (g5.lifecycle.start)", () => {
    state.vmStatus = "stopped";
    renderControl();
    expect(screen.getByRole("button", { name: "Preview" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Preview" })).toHaveAttribute(
      "title",
      "Start the workspace and launch a preview",
    );
  });

  it("still names why a stopping or unknown VM cannot preview, with no Start to offer", () => {
    state.vmStatus = "stopping";
    renderControl();
    const trigger = screen.getByRole("button", { name: "Preview" });
    expect(trigger).toBeDisabled();
    expect(trigger).toHaveAttribute(
      "title",
      expect.stringContaining("stopping"),
    );
  });

  it("opens the dialog with its run log on an Activity row's request, and again on the next request (g5.ui.activity.preview-row)", async () => {
    panel.props.length = 0;
    server.use(
      http.get("/api/v1/workspaces/workspace-1/preview", () =>
        HttpResponse.json({ data: RUNNING_PREVIEW }),
      ),
      http.get("/api/v1/tasks/preview-task-1/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e1", "invocation.started", {}),
              timelineEntry("e2", "module.preview.ready", { port: 3000 }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const user = userEvent.setup();
    // A request present at mount belongs to an earlier instance: it opens
    // nothing. One that arrives while mounted opens the dialog with its log.
    const { rerender } = renderControl({
      openRequest: { taskId: "preview-task-1", log: true, seq: 0 },
    });
    await screen.findByRole("button", { name: "Preview" });
    expect(screen.queryByTestId("workspace-preview-log-toggle")).toBeNull();
    rerender(
      <PreviewControl
        workspaceId="workspace-1"
        isDirty={false}
        openRequest={{ taskId: "preview-task-1", log: true, seq: 1 }}
      />,
    );
    const toggle = await screen.findByTestId("workspace-preview-log-toggle");
    await screen.findByTestId("workspace-preview-run-log");
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    // The owner closes the dialog; the same request stays quiet, a new one
    // reopens it without the log.
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("workspace-preview-log-toggle")).toBeNull();
    rerender(
      <PreviewControl
        workspaceId="workspace-1"
        isDirty={false}
        openRequest={{ taskId: "preview-task-1", log: true, seq: 1 }}
      />,
    );
    expect(screen.queryByTestId("workspace-preview-log-toggle")).toBeNull();
    rerender(
      <PreviewControl
        workspaceId="workspace-1"
        isDirty={false}
        openRequest={{ taskId: "preview-task-1", log: false, seq: 2 }}
      />,
    );
    expect(
      await screen.findByTestId("workspace-preview-log-toggle"),
    ).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("workspace-preview-run-log")).toBeNull();

    // A request for a different run than the current preview opens the
    // dialog without its log: the log belongs to the run the row named.
    await user.keyboard("{Escape}");
    rerender(
      <PreviewControl
        workspaceId="workspace-1"
        isDirty={false}
        openRequest={{ taskId: "older-task", log: true, seq: 5 }}
      />,
    );
    expect(
      await screen.findByTestId("workspace-preview-log-toggle"),
    ).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("workspace-preview-run-log")).toBeNull();
    await user.keyboard("{Escape}");
    rerender(
      <PreviewControl
        workspaceId="workspace-1"
        isDirty={false}
        openRequest={{ taskId: "preview-task-1", log: false, seq: 6 }}
      />,
    );
    await screen.findByTestId("workspace-preview-log-toggle");

    // A request that arrives while the dialog is already open still
    // applies its log (setOpen is a no-op then).
    rerender(
      <PreviewControl
        workspaceId="workspace-1"
        isDirty={false}
        openRequest={{ taskId: "preview-task-1", log: true, seq: 7 }}
      />,
    );
    await screen.findByTestId("workspace-preview-run-log");
    expect(screen.getByTestId("workspace-preview-log-toggle")).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });
});
