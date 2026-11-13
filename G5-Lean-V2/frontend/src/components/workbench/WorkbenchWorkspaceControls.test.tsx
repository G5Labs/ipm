import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type {
  AsyncState,
  OrgMemberDTO,
  WorkspaceVmView,
} from "../../lib/types";
import { createMockWorkspaceVm } from "../../test/fixtures";
import { ribbonButtonShape } from "./ActionRibbon";
import { WorkbenchWorkspaceControls } from "./WorkbenchWorkspaceControls";

const members: OrgMemberDTO[] = [
  {
    userId: "owner",
    displayName: "dvd-g5",
    email: "dvd@example.com",
    avatarUrl: null,
    role: "admin",
    deactivatedAt: null,
    activityRevision: 0,
    shutdownPending: false,
    joinedAt: "2026-10-01T00:00:00Z",
  },
  {
    userId: "zcl",
    displayName: "zcl",
    email: "zcl@example.com",
    avatarUrl: null,
    role: "member",
    deactivatedAt: null,
    activityRevision: 0,
    shutdownPending: false,
    joinedAt: "2026-10-01T00:00:00Z",
  },
  {
    userId: "rahul",
    displayName: "",
    email: "rahul@example.com",
    avatarUrl: null,
    role: "member",
    deactivatedAt: null,
    activityRevision: 0,
    shutdownPending: false,
    joinedAt: "2026-10-01T00:00:00Z",
  },
];

function vm(
  status: WorkspaceVmView["status"],
  extra: Partial<WorkspaceVmView> = {},
): AsyncState<WorkspaceVmView> {
  return { status: "ok", data: createMockWorkspaceVm({ status, ...extra }) };
}

function props(
  overrides: Partial<
    React.ComponentProps<typeof WorkbenchWorkspaceControls>
  > = {},
) {
  return {
    vm: vm("stopped"),
    ownerUserId: "owner",
    members,
    canControl: true,
    canTransfer: true,
    isBase: false,
    busy: false,
    onResume: vi.fn(),
    onStop: vi.fn(),
    onTransfer: vi.fn(),
    onOpenDetails: vi.fn(),
    ...overrides,
  };
}

// The ribbon's workspace controls (workbench g5.workbench.shell-chrome).
describe("WorkbenchWorkspaceControls", () => {
  it("says Start while stopped or failed, Stop while running, and only names a transition in between", () => {
    const { rerender } = render(<WorkbenchWorkspaceControls {...props()} />);
    expect(screen.getByTestId("workspace-vm-start")).toHaveTextContent("Start");
    expect(screen.getByTestId("workspace-vm-start")).toHaveAttribute(
      "title",
      "VM: Stopped",
    );
    rerender(<WorkbenchWorkspaceControls {...props({ vm: vm("failed") })} />);
    expect(screen.getByTestId("workspace-vm-start")).toHaveTextContent("Start");
    expect(screen.getByTestId("workspace-vm-start")).toHaveAttribute(
      "title",
      "VM: Failed",
    );
    rerender(<WorkbenchWorkspaceControls {...props({ vm: vm("running") })} />);
    expect(screen.queryByTestId("workspace-vm-start")).toBeNull();
    expect(screen.getByTestId("workspace-vm-stop")).toHaveTextContent("Stop");
    for (const [status, label] of [
      ["starting", "Starting…"],
      ["queued_for_capacity", "Waiting…"],
      ["stopping", "Stopping…"],
    ] as const) {
      rerender(
        <WorkbenchWorkspaceControls
          {...props({
            vm: vm(status, {
              lastError:
                status === "queued_for_capacity"
                  ? { reason: "host_capacity_refused", host_reason: "slots" }
                  : null,
            }),
          })}
        />,
      );
      const pending = screen.getByTestId("workspace-vm-pending");
      expect(pending).toHaveTextContent(label);
      expect(pending).toBeDisabled();
      // A queued start says why it waits in the button's tooltip; the row
      // never lays the reason out beside the buttons, so they stay put
      // (`g5.ui.capacity-wait-notice`).
      expect(screen.queryByTestId("workspace-vm-capacity-reason")).toBeNull();
      expect(screen.queryByText(/slot/i)).toBeNull();
      if (status === "queued_for_capacity") {
        expect(pending.getAttribute("title")).toMatch(
          /Waiting for capacity · .*slot/i,
        );
      }
    }
  });

  it("runs the one admitted action and refuses while busy", async () => {
    const onResume = vi.fn();
    const onStop = vi.fn();
    const { rerender } = render(
      <WorkbenchWorkspaceControls {...props({ onResume, onStop })} />,
    );
    await userEvent.click(screen.getByTestId("workspace-vm-start"));
    expect(onResume).toHaveBeenCalledTimes(1);
    rerender(
      <WorkbenchWorkspaceControls
        {...props({ vm: vm("running"), onResume, onStop })}
      />,
    );
    await userEvent.click(screen.getByTestId("workspace-vm-stop"));
    expect(onStop).toHaveBeenCalledTimes(1);
    rerender(
      <WorkbenchWorkspaceControls
        {...props({ vm: vm("running"), busy: true, onResume, onStop })}
      />,
    );
    expect(screen.getByTestId("workspace-vm-stop")).toBeDisabled();
  });

  it("shows the VM control to the owner only and nothing but Details for Base", () => {
    const { rerender } = render(
      <WorkbenchWorkspaceControls {...props({ canControl: false })} />,
    );
    expect(screen.queryByTestId("workspace-vm-start")).toBeNull();
    expect(screen.getByTestId("workspace-owner-trigger")).toHaveTextContent(
      "Owner: dvd-g5",
    );
    expect(screen.getByTestId("workspace-info-button")).toHaveTextContent(
      "Details",
    );
    rerender(<WorkbenchWorkspaceControls {...props({ isBase: true })} />);
    expect(screen.queryByTestId("workspace-owner-trigger")).toBeNull();
    expect(screen.queryByTestId("workspace-vm-start")).toBeNull();
    expect(screen.getByTestId("workspace-info-button")).toBeVisible();
  });

  it("opens Transfer ownership for people who may transfer, confirms inline and sends the pick", async () => {
    const onTransfer = vi.fn();
    render(<WorkbenchWorkspaceControls {...props({ onTransfer })} />);
    const trigger = screen.getByTestId("workspace-owner-trigger");
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(within(trigger).getByTestId("owner-key-icon")).toBeInTheDocument();
    await userEvent.click(trigger);
    const menu = screen.getByRole("menu", { name: "Transfer ownership" });
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(within(menu).getByText("dvd-g5")).toBeInTheDocument();
    // The owner is never offered; a member without a display name shows the email.
    const options = within(menu).getAllByTestId("workspace-owner-option");
    expect(options.map((o) => o.getAttribute("data-user-id"))).toEqual([
      "zcl",
      "rahul",
    ]);
    expect(options[1]).toHaveTextContent("rahul@example.com");
    expect(screen.queryByTestId("workspace-owner-transfer")).toBeNull();
    await userEvent.click(options[1]);
    const confirm = within(menu).getByTestId("workspace-owner-confirm");
    expect(confirm).toHaveTextContent(
      /Transfer this workspace to rahul@example.com/,
    );
    await userEvent.click(
      within(confirm).getByRole("button", { name: "Cancel" }),
    );
    expect(within(menu).queryByTestId("workspace-owner-confirm")).toBeNull();
    expect(onTransfer).not.toHaveBeenCalled();
    await userEvent.click(options[0]);
    await userEvent.click(within(menu).getByTestId("workspace-owner-transfer"));
    expect(onTransfer).toHaveBeenCalledWith("zcl");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes the menu on Escape or a click on a sibling control, and keeps a read-only owner badge for others", async () => {
    const { rerender } = render(<WorkbenchWorkspaceControls {...props()} />);
    await userEvent.click(screen.getByTestId("workspace-owner-trigger"));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    await userEvent.click(screen.getByTestId("workspace-owner-trigger"));
    await userEvent.click(screen.getAllByTestId("workspace-owner-option")[0]);
    expect(screen.getByTestId("workspace-owner-confirm")).toBeInTheDocument();
    // Details is a sibling, not part of the owner control.
    await userEvent.click(screen.getByTestId("workspace-info-button"));
    expect(screen.queryByRole("menu")).toBeNull();
    rerender(<WorkbenchWorkspaceControls {...props({ canTransfer: false })} />);
    const trigger = screen.getByTestId("workspace-owner-trigger");
    expect(trigger).toHaveTextContent("Owner: dvd-g5");
    expect(trigger).not.toHaveAttribute("aria-haspopup");
    expect(trigger).toBeDisabled();
  });

  it("keeps a long owner name on one truncating line under the icon, whole in the tooltip", () => {
    const longName = "Maximilian Alexander von Habsburg-Lothringen";
    const named = members.map((member) =>
      member.userId === "owner" ? { ...member, displayName: longName } : member,
    );
    const { rerender } = render(
      <WorkbenchWorkspaceControls {...props({ members: named })} />,
    );
    const trigger = screen.getByTestId("workspace-owner-trigger");
    expect(trigger).toHaveTextContent(`Owner: ${longName}`);
    expect(trigger).toHaveAttribute(
      "title",
      `Owner: ${longName} · Transfer ownership`,
    );
    const name = within(trigger).getByText(longName);
    expect(name).toHaveClass("truncate");
    expect(within(trigger).getByText("Owner:")).not.toBe(name);
    // Beside the icon the whole label truncates as one line.
    rerender(
      <WorkbenchWorkspaceControls
        {...props({ members: named, variant: "simplified" })}
      />,
    );
    expect(
      within(screen.getByTestId("workspace-owner-trigger")).getByText(
        `Owner: ${longName}`,
      ),
    ).toHaveClass("truncate");
  });

  it("takes the ribbon's button shape in every action-bar style and shows the Chat toggle on request", () => {
    const onToggle = vi.fn();
    for (const variant of [
      "extended-icons",
      "minimal-icons",
      "extended-text",
      "simplified",
      // The dynamic action bar's shrunk step.
      "compact-icons",
    ] as const) {
      const { unmount } = render(
        <WorkbenchWorkspaceControls
          {...props({
            variant,
            chatToggle: { open: false, onToggle },
          })}
        />,
      );
      const shape = ribbonButtonShape(variant, {});
      const details = screen.getByTestId("workspace-info-button");
      expect(details.className).toBe(shape.className);
      if (shape.labelHidden) {
        expect(details).toHaveAttribute("aria-label", "Details");
        expect(details).not.toHaveTextContent("Details");
      } else {
        expect(details).toHaveTextContent("Details");
      }
      // Icon-only styles prefix the tooltip with the name.
      expect(
        screen.getByTestId("toggle-right-panel").getAttribute("title"),
      ).toMatch(/Open chat$/);
      unmount();
    }
  });

  it("reports the last action error beside the controls", () => {
    render(
      <WorkbenchWorkspaceControls
        {...props({ error: "Transfer refused: the VM is flushing." })}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Transfer refused: the VM is flushing.",
    );
  });
});
