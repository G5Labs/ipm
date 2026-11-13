// TaskWorkspaceSwitchPrompt — cross-workspace task switch confirmation modal.
import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";

const mockUseTask = vi.fn();
vi.mock("../../hooks/useTasks", () => ({
  useTask: (taskId: string) => mockUseTask(taskId),
}));

import { TaskWorkspaceSwitchPrompt } from "./TaskWorkspaceSwitchPrompt";

function ok(data: Record<string, unknown>) {
  return { status: "ok" as const, data };
}

describe("TaskWorkspaceSwitchPrompt", () => {
  beforeEach(() => mockUseTask.mockReset());

  it("prompts to switch when the task is in another workspace, deep-linking on confirm", () => {
    mockUseTask.mockReturnValue(
      ok({
        id: "task-1",
        workspaceId: "ws-other",
        workspaceName: "compliance-service",
      }),
    );
    const onSwitch = vi.fn();
    render(
      <TaskWorkspaceSwitchPrompt
        taskId="task-1"
        currentWorkspaceId="ws-current"
        org="g5labs"
        onClose={vi.fn()}
        onSwitch={onSwitch}
        onOpenHere={vi.fn()}
      />,
    );

    expect(
      screen.getByTestId("task-workspace-switch-prompt"),
    ).toBeInTheDocument();
    expect(screen.getByText("compliance-service")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("task-workspace-switch-confirm"));
    expect(onSwitch).toHaveBeenCalledWith(
      "/g5labs/compliance-service/task/task-1",
    );
  });

  it("opens task output in place when the task belongs to the open workspace", () => {
    mockUseTask.mockReturnValue(
      ok({
        id: "task-1",
        workspaceId: "ws-current",
        workspaceName: "base",
      }),
    );
    const onOpenHere = vi.fn();
    render(
      <TaskWorkspaceSwitchPrompt
        taskId="task-1"
        currentWorkspaceId="ws-current"
        org="g5labs"
        onClose={vi.fn()}
        onSwitch={vi.fn()}
        onOpenHere={onOpenHere}
      />,
    );

    // the loaded task list omitted a same-workspace task — the prompt completes
    // the user's action (opens Workspace changes for it) rather than dropping it.
    expect(onOpenHere).toHaveBeenCalledWith("task-1");
    expect(
      screen.queryByTestId("task-workspace-switch-prompt"),
    ).not.toBeInTheDocument();
  });
});
