import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { TopBar } from "./TopBar";
import type { TopBarProps } from "./TopBar";

// The switcher fetches workspace data (React Query) and has its own coverage;
// stub the composite children so these tests exercise TopBar's own control
// gating without a QueryClientProvider.
vi.mock("./WorkspaceSwitcher", () => ({
  WorkspaceSwitcher: () => <div data-testid="workspace-switcher" />,
}));

function makeProps(overrides: Partial<TopBarProps> = {}): TopBarProps {
  return {
    workspace: null,
    orgId: "org-1",
    orgSlug: "acme",
    leftSidebarOpen: true,
    onToggleLeftSidebar: vi.fn(),
    rightPanelOpen: false,
    onToggleChat: vi.fn(),
    onOpenSettings: vi.fn(),
    onOpenOrgManagement: vi.fn(),
    onOpenNotifications: vi.fn(),
    onSelectWorkspace: vi.fn(),
    onNewWorkspace: vi.fn(),
    collaborators: [],
    unreadNotificationCount: 0,
    isMobile: false,
    ...overrides,
  };
}

describe("TopBar Org management button icon", () => {
  it("uses the CircleUserRound user icon (top-bar D11)", () => {
    render(<TopBar {...makeProps()} />);
    const button = screen.getByTestId("org-management-button");
    expect(button).toHaveAccessibleName("Org management");
    expect(button.querySelector("svg.lucide-circle-user-round")).not.toBeNull();
  });
});

describe("TopBar chat toggle", () => {
  it("hides the duplicate toggle when chat is hosted in the center panel", () => {
    render(
      <TopBar {...makeProps({ chatAvailable: true, showChatToggle: false })} />,
    );

    expect(screen.queryByTestId("toggle-right-panel")).not.toBeInTheDocument();
  });
});

describe("TopBar worker-host navigation", () => {
  it("calls the worker-host navigation callback", async () => {
    const onOpenWorkerHosts = vi.fn();
    render(<TopBar {...makeProps({ onOpenWorkerHosts })} />);

    const button = screen.getByRole("button", { name: "Workers" });
    await userEvent.click(button);
    expect(onOpenWorkerHosts).toHaveBeenCalledTimes(1);
  });

  it("omits the button when no navigation callback is supplied", () => {
    render(<TopBar {...makeProps()} />);
    expect(screen.queryByRole("button", { name: "Workers" })).toBeNull();
  });
});

describe("TopBar external issues", () => {
  it("hides the Activity entrypoint and primary chip when the caller passes neither", () => {
    render(<TopBar {...makeProps()} />);
    expect(screen.queryByTestId("top-bar-activity")).not.toBeInTheDocument();
    // Issues live in Workspace Details; the top bar has no Issues button.
    expect(screen.queryByTestId("top-bar-issues")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("top-bar-primary-issue"),
    ).not.toBeInTheDocument();
  });

  it("opens Activity from its entrypoint and hides it when not passed", async () => {
    const onOpenActivity = vi.fn();
    const { rerender } = render(<TopBar {...makeProps({ onOpenActivity })} />);
    const button = screen.getByTestId("top-bar-activity");
    expect(button).toHaveAccessibleName("Activity");
    await userEvent.click(button);
    expect(onOpenActivity).toHaveBeenCalledTimes(1);
    rerender(<TopBar {...makeProps({})} />);
    expect(screen.queryByTestId("top-bar-activity")).not.toBeInTheDocument();
  });

  it("shows the primary issue chip after the workspace switcher and opens the issue", async () => {
    const onOpen = vi.fn();
    render(
      <TopBar
        {...makeProps({
          onOpenActivity: vi.fn(),
          primaryIssue: {
            displayKey: "acme/api#12",
            title: "Search returns stale results",
            state: "open",
            onOpen,
          },
        })}
      />,
    );
    const chip = screen.getByTestId("top-bar-primary-issue");
    expect(chip).toHaveTextContent("acme/api#12");
    expect(screen.getByTestId("top-bar-primary-issue-state")).toHaveTextContent(
      "Open",
    );
    expect(chip).toHaveAccessibleName(
      "Primary issue acme/api#12, Open, Search returns stale results",
    );
    expect(
      screen.getByTestId("workspace-switcher").compareDocumentPosition(chip) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await userEvent.click(chip);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("omits the state when the live summary is unknown", () => {
    render(
      <TopBar
        {...makeProps({
          primaryIssue: {
            displayKey: "acme/api#12",
            title: null,
            state: null,
            onOpen: vi.fn(),
          },
        })}
      />,
    );
    expect(screen.getByTestId("top-bar-primary-issue")).toHaveTextContent(
      "acme/api#12",
    );
    expect(
      screen.queryByTestId("top-bar-primary-issue-state"),
    ).not.toBeInTheDocument();
  });

  it("does not render the chip in the mobile header", () => {
    render(
      <TopBar
        {...makeProps({
          isMobile: true,
          onOpenActivity: vi.fn(),
          primaryIssue: {
            displayKey: "acme/api#12",
            title: null,
            state: "open",
            onOpen: vi.fn(),
          },
        })}
      />,
    );
    expect(
      screen.queryByTestId("top-bar-primary-issue"),
    ).not.toBeInTheDocument();
  });
});
