import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import type { TerminalViewerState } from "../execution/terminalViewer";

const panel = vi.hoisted(() => ({
  props: [] as Array<Record<string, unknown>>,
  viewer: null as ((state: TerminalViewerState) => void) | null,
}));

vi.mock("../execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: (props: Record<string, unknown>) => {
    panel.props.push(props);
    panel.viewer =
      (props.onViewerState as
        | ((state: TerminalViewerState) => void)
        | undefined) ?? null;
    return (
      <div
        data-testid="mock-terminal-panel"
        data-terminal-id={String(props.terminalId)}
        data-read-only={String(props.readOnly === true)}
        data-attach={String(props.attach !== false)}
        data-pending={(props.pendingStateText as string | null) ?? ""}
      />
    );
  },
}));

import { PreviewRunLog } from "./PreviewRunLog";

function timelineEntry(
  id: string,
  kind: string,
  payload: Record<string, unknown>,
) {
  return {
    id,
    entryType: "checkpoint",
    content: kind,
    metadata: { kind, payload },
    createdAt: `2026-10-20T10:42:0${id.length}Z`,
  };
}

function renderLog(
  view: {
    status: string;
    ready: boolean;
    closing: boolean;
    close: string | null;
  },
  vmRunning: boolean,
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <PreviewRunLog
        workspaceId="workspace-1"
        taskId="preview-task-9"
        view={{ ...view, port: 3000, profile: "full-stack" }}
        vmRunning={vmRunning}
        pollIntervalMs={60_000}
      />
    </QueryClientProvider>,
  );
}

describe("PreviewRunLog (g5.preview.run-log)", () => {
  beforeEach(() => {
    panel.props.length = 0;
    panel.viewer = null;
  });

  it("shows the stages from the view and lifecycle entries with the live read-only terminal below", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [timelineEntry("e1", "invocation.started", {})],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    renderLog(
      { status: "running", ready: false, closing: false, close: null },
      true,
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-preview-stage-building"),
      ).toHaveAttribute("data-state", "current"),
    );
    expect(
      screen.getByTestId("workspace-preview-stage-starting"),
    ).toHaveAttribute("data-state", "done");
    expect(
      screen.getByTestId("workspace-preview-stage-serving"),
    ).toHaveAttribute("data-state", "pending");
    const mounted = screen.getByTestId("mock-terminal-panel");
    expect(mounted).toHaveAttribute(
      "data-terminal-id",
      "preview:preview-task-9",
    );
    expect(mounted).toHaveAttribute("data-read-only", "true");
    expect(panel.props[0]).toMatchObject({
      workspaceId: "workspace-1",
      enabled: true,
      sessionLabel: "preview · full-stack",
    });
    expect(
      screen.queryByTestId("workspace-preview-output-unavailable"),
    ).toBeNull();
    expect(screen.queryByTestId("workspace-preview-run-failure")).toBeNull();
  });

  it("marks closing current while the shutdown and cleanup stream", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
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
    renderLog(
      { status: "running", ready: true, closing: true, close: null },
      true,
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-preview-stage-closing"),
      ).toHaveAttribute("data-state", "current"),
    );
    expect(
      screen.getByTestId("workspace-preview-stage-serving"),
    ).toHaveAttribute("data-state", "done");
    expect(screen.getByTestId("mock-terminal-panel")).toBeInTheDocument();
  });

  it("shows a failed run's retained output while the VM runs, then the exit status and stderr tail", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e1", "invocation.started", {}),
              timelineEntry("e2", "invocation.outcome", {
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
    renderLog(
      { status: "failed", ready: false, closing: false, close: null },
      true,
    );
    const failure = await screen.findByTestId("workspace-preview-run-failure");
    expect(failure).toHaveTextContent("Application exited with status 7");
    expect(
      screen.getByTestId("workspace-preview-stderr-tail"),
    ).toHaveTextContent("Error: listen EADDRINUSE :3000");
    expect(
      screen.getByTestId("workspace-preview-stage-building"),
    ).toHaveAttribute("data-state", "failed");
    expect(
      screen.getByTestId("workspace-preview-stage-closed"),
    ).toHaveTextContent("Failed");
    expect(screen.getByTestId("mock-terminal-panel")).toBeInTheDocument();

    // The guest no longer hosts the session (an older dispatcher, a replaced
    // VM): the diagnostics-only view takes the terminal's place.
    act(() => {
      panel.viewer?.({
        status: "Session unavailable",
        offset: null,
        received: 0,
        ended: false,
        superseded: false,
        rejected: true,
      });
    });
    expect(screen.queryByTestId("mock-terminal-panel")).toBeNull();
    // The VM runs, so the notice says the session is gone, not that the VM
    // stopped.
    expect(
      screen.getByTestId("workspace-preview-output-unavailable"),
    ).toHaveTextContent("the running VM no longer hosts this run's session");
    expect(
      screen.getByTestId("workspace-preview-stderr-tail"),
    ).toBeInTheDocument();
  });

  it("holds a live run's first attach until the timeline says the module started, on one mounted panel", async () => {
    let entries: unknown[] = [];
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: { entries, cursor: null, hasMore: false },
        }),
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const log = (status: string) => (
      <QueryClientProvider client={client}>
        <PreviewRunLog
          workspaceId="workspace-1"
          taskId="preview-task-9"
          view={{
            status,
            ready: false,
            closing: false,
            close: null,
            port: 3000,
          }}
          vmRunning
          pollIntervalMs={60_000}
          retryDelayMs={30}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(log("queued"));
    // Opened right after Start: the guest cannot host the session yet, so
    // the panel is mounted but holds its attach, and says why.
    const mounted = await screen.findByTestId("mock-terminal-panel");
    expect(mounted).toHaveAttribute("data-attach", "false");
    expect(mounted).toHaveAttribute(
      "data-pending",
      "waiting for the application to start…",
    );
    expect(
      screen.queryByTestId("workspace-preview-output-unavailable"),
    ).toBeNull();
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-preview-stage-starting"),
      ).toHaveAttribute("data-state", "current"),
    );
    // The task was dispatched and the module started: the next timeline
    // read (invalidated by the view change) releases the attach, on the
    // same panel instance.
    entries = [timelineEntry("e1", "invocation.started", {})];
    rerender(log("running"));
    await waitFor(() =>
      expect(screen.getByTestId("mock-terminal-panel")).toHaveAttribute(
        "data-attach",
        "true",
      ),
    );
    expect(screen.getByTestId("mock-terminal-panel")).toBe(mounted);
    // Still waiting until the attach connects; then the live words take over.
    expect(mounted).toHaveAttribute(
      "data-pending",
      "waiting for the application to start…",
    );
    act(() => {
      panel.viewer?.({
        status: "Connected",
        offset: 0,
        received: 0,
        ended: false,
        superseded: false,
        rejected: false,
      });
    });
    expect(screen.getByTestId("mock-terminal-panel")).toHaveAttribute(
      "data-pending",
      "",
    );
    expect(screen.getByTestId("mock-terminal-panel")).toBe(mounted);
  });

  it("keeps an attached viewer attached while a timeline refresh fails, and does not re-hold a started run", async () => {
    let fail = false;
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        fail
          ? HttpResponse.json({ error: "unavailable" }, { status: 503 })
          : HttpResponse.json({
              data: {
                entries: [timelineEntry("e1", "invocation.started", {})],
                cursor: null,
                hasMore: false,
              },
            }),
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const log = (ready: boolean) => (
      <QueryClientProvider client={client}>
        <PreviewRunLog
          workspaceId="workspace-1"
          taskId="preview-task-9"
          view={{
            status: "running",
            ready,
            closing: false,
            close: null,
            port: 3000,
          }}
          vmRunning
          pollIntervalMs={60_000}
          retryDelayMs={30}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(log(false));
    const mounted = screen.getByTestId("mock-terminal-panel");
    await waitFor(() => expect(mounted).toHaveAttribute("data-attach", "true"));
    act(() => {
      panel.viewer?.({
        status: "Connected",
        offset: 0,
        received: 0,
        ended: false,
        superseded: false,
        rejected: false,
      });
    });
    // The next refresh fails (a view change invalidates the timeline): the
    // entries are gone for a moment, the attachment is not.
    fail = true;
    rerender(log(false));
    await act(async () => {
      await client.invalidateQueries({
        queryKey: ["task-timeline", "preview-task-9"],
      });
    });
    await screen.findByRole("alert");
    expect(screen.getByTestId("mock-terminal-panel")).toBe(mounted);
    expect(mounted).toHaveAttribute("data-attach", "true");
    expect(mounted).toHaveAttribute("data-pending", "");
    // The same failure before any attach would not re-hold the run either:
    // once started, started.
    act(() => {
      panel.viewer?.({
        status: "Session unavailable",
        offset: null,
        received: 0,
        ended: false,
        superseded: false,
        rejected: true,
      });
    });
    expect(mounted).toHaveAttribute("data-attach", "false");
    await waitFor(() => expect(mounted).toHaveAttribute("data-attach", "true"));
  });

  it("retries a refused attach while the run is live without rebuilding the panel", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [timelineEntry("e1", "invocation.started", {})],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <PreviewRunLog
          workspaceId="workspace-1"
          taskId="preview-task-9"
          view={{
            status: "running",
            ready: false,
            closing: false,
            close: null,
            port: 3000,
          }}
          vmRunning
          pollIntervalMs={60_000}
          retryDelayMs={30}
        />
      </QueryClientProvider>,
    );
    const mounted = screen.getByTestId("mock-terminal-panel");
    await waitFor(() => expect(mounted).toHaveAttribute("data-attach", "true"));
    // The guest opened the PTY a moment after the entry: this attach is
    // refused. The panel stays, with its attach held for the retry delay.
    act(() => {
      panel.viewer?.({
        status: "Session unavailable",
        offset: null,
        received: 0,
        ended: false,
        superseded: false,
        rejected: true,
      });
    });
    expect(screen.getByTestId("mock-terminal-panel")).toBe(mounted);
    expect(mounted).toHaveAttribute("data-attach", "false");
    expect(mounted).toHaveAttribute(
      "data-pending",
      "waiting for the application to start…",
    );
    expect(
      screen.queryByTestId("workspace-preview-output-unavailable"),
    ).toBeNull();
    // After the delay the same panel attaches again.
    await waitFor(() => expect(mounted).toHaveAttribute("data-attach", "true"));
    expect(screen.getByTestId("mock-terminal-panel")).toBe(mounted);
  });

  it("keeps a retried attach's retained output mounted after the run fails", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [timelineEntry("e1", "invocation.started", {})],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const log = (status: string) => (
      <QueryClientProvider client={client}>
        <PreviewRunLog
          workspaceId="workspace-1"
          taskId="preview-task-9"
          view={{
            status,
            ready: false,
            closing: false,
            close: null,
            port: 3000,
          }}
          vmRunning
          pollIntervalMs={60_000}
          retryDelayMs={30}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(log("running"));
    const mounted = screen.getByTestId("mock-terminal-panel");
    await waitFor(() => expect(mounted).toHaveAttribute("data-attach", "true"));
    act(() => {
      panel.viewer?.({
        status: "Session unavailable",
        offset: null,
        received: 0,
        ended: false,
        superseded: false,
        rejected: true,
      });
    });
    expect(mounted).toHaveAttribute("data-attach", "false");
    await waitFor(() => expect(mounted).toHaveAttribute("data-attach", "true"));
    // The retry attached once the guest hosted the session.
    act(() => {
      panel.viewer?.({
        status: "Connected",
        offset: 0,
        received: 0,
        ended: false,
        superseded: false,
        rejected: false,
      });
    });
    // The run then fails while the VM runs: the ended session's retained
    // output stays, exactly as for an attach that was never refused, on the
    // same DOM node.
    rerender(log("failed"));
    expect(screen.getByTestId("mock-terminal-panel")).toBe(mounted);
    expect(mounted).toHaveAttribute("data-attach", "true");
    expect(mounted).toHaveAttribute("data-pending", "");
    expect(
      screen.queryByTestId("workspace-preview-output-unavailable"),
    ).toBeNull();
  });

  it("shows the notice once a live run's bounded retries are spent (an older dispatcher never hosts the session)", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [timelineEntry("e1", "invocation.started", {})],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <PreviewRunLog
          workspaceId="workspace-1"
          taskId="preview-task-9"
          view={{
            status: "running",
            ready: false,
            closing: false,
            close: null,
            port: 3000,
          }}
          vmRunning
          pollIntervalMs={60_000}
          retryDelayMs={1}
        />
      </QueryClientProvider>,
    );
    const mounted = screen.getByTestId("mock-terminal-panel");
    const refuse = () =>
      act(() => {
        panel.viewer?.({
          status: "Session unavailable",
          offset: null,
          received: 0,
          ended: false,
          superseded: false,
          rejected: true,
        });
      });
    // 24 refusals are retried, each on the same panel.
    for (let attempt = 1; attempt <= 24; attempt += 1) {
      await waitFor(() =>
        expect(mounted).toHaveAttribute("data-attach", "true"),
      );
      refuse();
      expect(screen.getByTestId("mock-terminal-panel")).toBe(mounted);
      expect(mounted).toHaveAttribute("data-attach", "false");
    }
    // The 25th is the end of the budget: the diagnostics-only notice for a
    // running VM takes the terminal's place.
    await waitFor(() => expect(mounted).toHaveAttribute("data-attach", "true"));
    refuse();
    expect(screen.queryByTestId("mock-terminal-panel")).toBeNull();
    expect(
      screen.getByTestId("workspace-preview-output-unavailable"),
    ).toHaveTextContent("the running VM no longer hosts this run's session");
  });

  it("treats an ended run's refused attach as final: the notice at once", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [timelineEntry("e1", "invocation.started", {})],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    renderLog(
      { status: "failed", ready: false, closing: false, close: null },
      true,
    );
    // An ended run attaches at once (its session is whatever the guest
    // retained); a refusal is final.
    const mounted = await screen.findByTestId("mock-terminal-panel");
    expect(mounted).toHaveAttribute("data-attach", "true");
    expect(mounted).toHaveAttribute("data-pending", "");
    act(() => {
      panel.viewer?.({
        status: "Session unavailable",
        offset: null,
        received: 0,
        ended: false,
        superseded: false,
        rejected: true,
      });
    });
    expect(screen.queryByTestId("mock-terminal-panel")).toBeNull();
    expect(
      screen.getByTestId("workspace-preview-output-unavailable"),
    ).toHaveTextContent("the running VM no longer hosts this run's session");
  });

  it("offers no terminal once the VM stopped: stages, the notice and the tail only", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e1", "invocation.started", {}),
              timelineEntry("e2", "invocation.outcome", {
                status: "cancelled",
                detail: { close: "killed" },
              }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    renderLog(
      { status: "cancelled", ready: false, closing: false, close: "killed" },
      false,
    );
    const failure = await screen.findByTestId("workspace-preview-run-failure");
    expect(failure).toHaveTextContent(
      "Closed, but cleanup did not finish in time and was killed",
    );
    expect(screen.queryByTestId("mock-terminal-panel")).toBeNull();
    expect(
      screen.getByTestId("workspace-preview-output-unavailable"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("workspace-preview-stage-closed"),
    ).toHaveTextContent("Closed; cleanup failed");
  });
  it("notices a timed-out or interrupted run in the Activity row's words (g5.ui.activity.preview-row)", async () => {
    server.use(
      http.get("/api/v1/tasks/preview-task-9/timeline", () =>
        HttpResponse.json({
          data: {
            entries: [
              timelineEntry("e1", "invocation.started", {}),
              timelineEntry("e2", "invocation.outcome", {
                status: "timed_out",
                detail: {},
              }),
            ],
            cursor: null,
            hasMore: false,
          },
        }),
      ),
    );
    renderLog(
      { status: "failed", ready: false, closing: false, close: null },
      false,
    );
    const failure = await screen.findByTestId("workspace-preview-run-failure");
    expect(failure).toHaveTextContent("Exceeded its time limit");
    expect(screen.getByText("Failed while building")).toBeInTheDocument();
  });
});
