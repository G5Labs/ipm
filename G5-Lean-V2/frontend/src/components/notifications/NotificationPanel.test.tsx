/**
 * NotificationPanel — review activity has its own group, so a pushed
 * round or a source that moved on is not filed under "Completed".
 */

import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { renderApp } from "../../test/render";
import { createMockNotification } from "../../test/fixtures";
import { NotificationPanel } from "./NotificationPanel";

describe("NotificationPanel", () => {
  it("files review notices under Review, apart from Completed", () => {
    renderApp(
      <NotificationPanel
        notifications={[
          createMockNotification({
            id: "n-review",
            category: "review",
            title: "kaihui-test3 has a new commit",
            message: "Update your review copy to review it.",
          }),
          createMockNotification({
            id: "n-done",
            category: "worker_terminated",
            title: "Worker stopped",
            message: "Done.",
          }),
        ]}
        onAction={vi.fn()}
        onClearAll={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    const reviewGroup = screen.getByTestId("notification-group-review");
    expect(reviewGroup).toHaveTextContent("Review");
    expect(
      within(reviewGroup).getByText("kaihui-test3 has a new commit"),
    ).toBeInTheDocument();
    const completedGroup = screen.getByTestId("notification-group-completed");
    expect(
      within(completedGroup).queryByText("kaihui-test3 has a new commit"),
    ).toBeNull();
    expect(
      within(completedGroup).getByText("Worker stopped"),
    ).toBeInTheDocument();
  });
});
