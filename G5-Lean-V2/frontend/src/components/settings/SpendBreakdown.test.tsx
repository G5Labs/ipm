import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { SpendBreakdown } from "./SpendBreakdown";
import type { TaskCostBreakdown } from "../../lib/types";

const byTask: TaskCostBreakdown[] = [
  { taskId: "task-1", taskName: "sync main", taskType: "sync", costUsd: 0.8 },
  {
    taskId: "task-2",
    taskName: "chat session",
    taskType: "chat",
    costUsd: 2.5,
  },
  {
    taskId: "task-3",
    taskName: "merge base",
    taskType: "merge",
    costUsd: 0.004,
  },
];

/** Task-name cells (the `.truncate` span in each data row) in DOM order. */
function renderedTaskOrder(): string[] {
  return Array.from(document.querySelectorAll("span.truncate")).map(
    (el) => el.textContent ?? "",
  );
}

describe("SpendBreakdown", () => {
  it("renders the empty state when the task list is not a list (issue #733)", () => {
    // A stale page's stats shape reached the table as `undefined` and the
    // spread threw "e is not iterable" into the page's error boundary.
    render(
      <SpendBreakdown
        byTask={undefined as unknown as TaskCostBreakdown[]}
        totalCostUsd={0}
      />,
    );
    expect(screen.getByText(/no spend data/i)).toBeInTheDocument();
  });

  it("shows conversation costs when no retained tasks exist", () => {
    render(<SpendBreakdown byTask={[]} totalCostUsd={2.5} />);
    expect(screen.getByText("Other usage")).toBeInTheDocument();
    expect(screen.getByText("$2.50")).toBeInTheDocument();
    expect(screen.queryByText(/no spend data/i)).not.toBeInTheDocument();
  });

  it("separates conversation costs without counting task costs twice", () => {
    render(<SpendBreakdown byTask={byTask} totalCostUsd={5.304} />);
    expect(
      within(screen.getByTestId("other-spend")).getByText("$2.00"),
    ).toBeInTheDocument();
    expect(renderedTaskOrder()).toEqual([
      "chat session",
      "sync main",
      "merge base",
    ]);
  });

  it("does not show floating-point rounding as conversation spend", () => {
    render(
      <SpendBreakdown
        byTask={[
          { ...byTask[0], costUsd: 0.1 },
          { ...byTask[1], costUsd: 0.2 },
        ]}
        totalCostUsd={0.3}
      />,
    );
    expect(screen.queryByTestId("other-spend")).not.toBeInTheDocument();
  });

  it("renders one row per VM with its formatted cost", () => {
    render(<SpendBreakdown byTask={byTask} />);
    expect(screen.getByText("sync main")).toBeInTheDocument();
    expect(screen.getByText("$0.80")).toBeInTheDocument();
    expect(screen.getByText("chat session")).toBeInTheDocument();
    expect(screen.getByText("$2.50")).toBeInTheDocument();
    expect(screen.getByText("merge base")).toBeInTheDocument();
    // Sub-cent costs format as "<$0.01" via formatCost.
    expect(screen.getByText("<$0.01")).toBeInTheDocument();
  });

  it("defaults to descending cost order and toggles ascending on header click", () => {
    render(<SpendBreakdown byTask={byTask} />);
    // Default: highest cost first (2.50, 0.80, 0.05).
    expect(renderedTaskOrder()).toEqual([
      "chat session",
      "sync main",
      "merge base",
    ]);

    fireEvent.click(screen.getByRole("button", { name: /sort by cost/i }));

    // Ascending: lowest cost first.
    expect(renderedTaskOrder()).toEqual([
      "merge base",
      "sync main",
      "chat session",
    ]);
  });

  it("shows an empty state when there is no per-task spend", () => {
    render(<SpendBreakdown byTask={[]} />);
    expect(
      screen.getByText(/no spend data for this workspace/i),
    ).toBeInTheDocument();
    // No "By Task" table header is rendered in the empty state.
    expect(screen.queryByText("Task Name")).not.toBeInTheDocument();
  });

  it("renders each task's type", () => {
    render(<SpendBreakdown byTask={byTask} />);
    const wrapper = screen.getByText("By Task").parentElement as HTMLElement;
    expect(within(wrapper).getByText("sync")).toBeInTheDocument();
    expect(within(wrapper).getByText("chat")).toBeInTheDocument();
    expect(within(wrapper).getByText("merge")).toBeInTheDocument();
  });
});
