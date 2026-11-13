import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ShellControls } from "./ShellControls";

vi.mock("../platform-issues/PlatformIssueReportButton", () => ({
  PlatformIssueReportTrigger: ({ placement }: { placement: string }) => (
    <button data-testid="report-stub" data-placement={placement} />
  ),
}));

// The strip's global controls (workbench g5.workbench.shell-chrome).
describe("ShellControls", () => {
  it("renders only the controls the page supplies, with the title bar's ids", async () => {
    const onOpenSettings = vi.fn();
    const onOpenAccount = vi.fn();
    const onOpen = vi.fn();
    render(
      <ShellControls
        notifications={{ unreadCount: 3, onOpen }}
        onOpenSettings={onOpenSettings}
        onOpenAccount={onOpenAccount}
      />,
    );
    expect(screen.queryByTestId("top-bar-worker-hosts")).toBeNull();
    expect(screen.getByTestId("report-stub")).toHaveAttribute(
      "data-placement",
      "topbar",
    );
    const bell = screen.getByTestId("top-bar-notifications");
    expect(bell).toHaveTextContent("3");
    await userEvent.click(bell);
    expect(onOpen).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByTestId("settings-button"));
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByTestId("org-management-button"));
    expect(onOpenAccount).toHaveBeenCalledTimes(1);
  });

  it("offers Workers only to people who manage them", async () => {
    const onOpenWorkerHosts = vi.fn();
    render(<ShellControls onOpenWorkerHosts={onOpenWorkerHosts} />);
    await userEvent.click(screen.getByTestId("top-bar-worker-hosts"));
    expect(onOpenWorkerHosts).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("top-bar-notifications")).toBeNull();
    expect(screen.queryByTestId("settings-button")).toBeNull();
    expect(screen.queryByTestId("org-management-button")).toBeNull();
  });
});
