import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TaskRow } from "./TreeNode";
import type { TaskSummary } from "../../lib/types";

function waitingTask(overrides: Partial<TaskSummary> = {}): TaskSummary {
  return {
    id: "t1",
    name: "chat task",
    taskType: "code",
    status: "waiting",
    workspaceId: "w1",
    workspaceName: "WS",
    specContext: null,
    dependsOn: [],
    filesChanged: [],
    costUsd: 0,
    sessionId: "s1",
    assignedAt: null,
    completedAt: null,
    ...overrides,
  } as TaskSummary;
}

describe("TaskRow canonical task status labels", () => {
  it("labels a held task as waiting to continue", () => {
    render(
      <TaskRow
        task={waitingTask()}
        isSelected={false}
        onSelectTask={() => {}}
      />,
    );
    expect(screen.getByLabelText("Waiting to continue")).toBeInTheDocument();
  });

  it("keeps waiting authoritative when execution-attempt metadata is present", () => {
    render(
      <TaskRow
        task={waitingTask({
          latestAttempt: {
            id: "attempt-1",
            attemptNumber: 1,
            provider: "claude_code",
            providerSessionId: "provider-session-1",
            disposition: "waiting",
            reason: "awaiting_user_input",
            retryPolicy: null,
            lifecyclePosture: "action_required",
            startedAt: "2026-08-04T01:58:00Z",
            endedAt: null,
          },
        })}
        isSelected={false}
        onSelectTask={() => {}}
      />,
    );
    expect(screen.getByLabelText("Waiting to continue")).toBeInTheDocument();
  });
  it("renders queued, running, and terminal canonical states", () => {
    const { rerender } = render(
      <TaskRow
        task={waitingTask({ status: "queued" })}
        isSelected={false}
        onSelectTask={() => {}}
      />,
    );
    expect(screen.getByLabelText("Queued")).toBeInTheDocument();

    rerender(
      <TaskRow
        task={waitingTask({ status: "running" })}
        isSelected={false}
        onSelectTask={() => {}}
      />,
    );
    expect(screen.getByLabelText("Running")).toBeInTheDocument();

    for (const [status, label] of [
      ["succeeded", "Succeeded"],
      ["failed", "Failed"],
      ["cancelled", "Cancelled"],
    ] as const) {
      rerender(
        <TaskRow
          task={waitingTask({ status })}
          isSelected={false}
          onSelectTask={() => {}}
        />,
      );
      expect(screen.getByLabelText(label)).toBeInTheDocument();
    }
  });

  it("shows why a failed task failed on its row", () => {
    const { rerender } = render(
      <TaskRow
        task={{
          ...waitingTask({ status: "failed" }),
          failure: {
            reason: "dispatch_admission_failed",
            detail: "Conflict: permission mode refused",
          },
        }}
        isSelected={false}
        onSelectTask={() => {}}
      />,
    );
    expect(screen.getByTestId("tree-task-row")).toHaveAttribute(
      "title",
      "Failed: Conflict: permission mode refused",
    );

    rerender(
      <TaskRow
        task={{
          ...waitingTask({ status: "failed" }),
          failure: { reason: "idle_stop_failed" },
        }}
        isSelected={false}
        onSelectTask={() => {}}
      />,
    );
    expect(screen.getByTestId("tree-task-row")).toHaveAttribute(
      "title",
      "Failed: idle_stop_failed",
    );

    rerender(
      <TaskRow
        task={waitingTask({ status: "succeeded" })}
        isSelected={false}
        onSelectTask={() => {}}
      />,
    );
    expect(screen.getByTestId("tree-task-row")).not.toHaveAttribute("title");
  });
});
