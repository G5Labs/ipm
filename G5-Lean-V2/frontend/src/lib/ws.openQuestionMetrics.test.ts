import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { WebSocketManager, type WSEvent } from "./ws";

function makeManager() {
  const queryClient = new QueryClient();
  const invalidateQueries = vi.spyOn(queryClient, "invalidateQueries");
  // `setQueriesData` backs the terminal task.status chat-session close path;
  // a no-op keeps the handler runnable under this focused invalidation mock.
  const setQueriesData = vi.fn();
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: async () => "ticket",
    queryClient: Object.assign(queryClient, { setQueriesData }),
  });
  return { manager, invalidateQueries };
}

function emit(manager: WebSocketManager, event: WSEvent): void {
  (manager as unknown as { handleEvent: (event: WSEvent) => void }).handleEvent(
    event,
  );
}

function invalidatedOpenQuestionMetrics(
  invalidateQueries: ReturnType<typeof makeManager>["invalidateQueries"],
): boolean {
  return invalidateQueries.mock.calls.some((call) => {
    const options = call[0] as { queryKey?: unknown[] };
    return options?.queryKey?.[0] === "workspace-open-question-metrics";
  });
}

describe("Changes report refresh on discrete doc events", () => {
  it("invalidates the workspace-changes report for the doc's workspace", () => {
    for (const type of [
      "doc.created",
      "doc.content_changed",
      "doc.deleted",
    ] as const) {
      const { manager, invalidateQueries } = makeManager();
      emit(manager, { type, workspaceId: "workspace-1", docId: "doc-1" });
      const invalidatedChanges = invalidateQueries.mock.calls.some((call) => {
        const options = call[0] as { queryKey?: unknown[] };
        return (
          options?.queryKey?.[0] === "workspace-changes" &&
          options?.queryKey?.[1] === "workspace-1"
        );
      });
      expect(invalidatedChanges, type).toBe(true);
    }
  });
});

describe("open-question metrics refresh on discrete doc events", () => {
  it("invalidates the metrics family when a Project Doc is created", () => {
    const { manager, invalidateQueries } = makeManager();

    emit(manager, {
      type: "doc.created",
      workspaceId: "workspace-1",
      docId: "doc-1",
    });

    expect(invalidatedOpenQuestionMetrics(invalidateQueries)).toBe(true);
  });

  it("invalidates the metrics family when Project Doc content changes", () => {
    const { manager, invalidateQueries } = makeManager();

    emit(manager, {
      type: "doc.content_changed",
      workspaceId: "workspace-1",
      docId: "doc-1",
    });

    expect(invalidatedOpenQuestionMetrics(invalidateQueries)).toBe(true);
  });

  it("invalidates the metrics family when a Project Doc is deleted", () => {
    const { manager, invalidateQueries } = makeManager();

    emit(manager, {
      type: "doc.deleted",
      workspaceId: "workspace-1",
      docId: "doc-1",
    });

    expect(invalidatedOpenQuestionMetrics(invalidateQueries)).toBe(true);
  });

  it("refreshes the docs list for the owning workspace on doc events", () => {
    const { manager, invalidateQueries } = makeManager();

    emit(manager, {
      type: "doc.created",
      workspaceId: "workspace-1",
      docId: "doc-1",
    });

    expect(
      invalidateQueries.mock.calls.some((call) => {
        const options = call[0] as { queryKey?: unknown[] };
        return (
          options?.queryKey?.[0] === "docs" &&
          options?.queryKey?.[1] === "workspace-1"
        );
      }),
    ).toBe(true);
  });

  // Terminal tasks can change workspace attention counts; active transitions
  // do not and must not refetch.
  it("invalidates the metrics family only for terminal task.status transitions", () => {
    const terminal = makeManager();
    emit(terminal.manager, {
      type: "task.status",
      taskId: "task-1",
      status: "succeeded",
    });
    expect(invalidatedOpenQuestionMetrics(terminal.invalidateQueries)).toBe(
      true,
    );

    const active = makeManager();
    emit(active.manager, {
      type: "task.status",
      taskId: "task-1",
      status: "running",
    });
    expect(invalidatedOpenQuestionMetrics(active.invalidateQueries)).toBe(
      false,
    );
  });
});
