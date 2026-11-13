import { fireEvent, render, screen } from "@testing-library/react";
import { createMockWorkspaceVm } from "../../test/fixtures";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type {
  AsyncState,
  OrgMemberDTO,
  WorkspaceVmView,
} from "../../lib/types";
import { WorkspaceVmStatusStrip } from "./WorkspaceVmStatusStrip";
import { liveGateKind, liveGateReason } from "../../lib/workspaceLiveGate";

const members: OrgMemberDTO[] = [
  {
    userId: "owner",
    displayName: "Zhengchun Liu",
    email: "owner@example.com",
    avatarUrl: null,
    role: "admin",
    deactivatedAt: null,
    activityRevision: 0,
    shutdownPending: false,
    joinedAt: "2026-09-23T00:00:00Z",
  },
  {
    userId: "member",
    displayName: "Another workspace member with a long display name",
    email: "member@example.com",
    avatarUrl: null,
    role: "member",
    deactivatedAt: null,
    activityRevision: 0,
    shutdownPending: false,
    joinedAt: "2026-09-23T00:00:00Z",
  },
];

const vmData: WorkspaceVmView = createMockWorkspaceVm({
  imageRef: "registry.example.com/workspace:latest",
  lastFlushId: "d8097741-full-flush-id",
  unflushedLossAt: "2026-09-23T00:00:00Z",
});

function props(status: WorkspaceVmView["status"] = "stopped") {
  return {
    vm: {
      status: "ok",
      data: { ...vmData, status },
    } as AsyncState<WorkspaceVmView>,
    ownerUserId: "owner",
    members,
    canControl: true,
    canTransfer: true,
    isBase: false,
    busy: false,
    onResume: vi.fn(),
    onStop: vi.fn(),
    onOpenTerminal: vi.fn(),
    onTransfer: vi.fn(),
  };
}

describe("WorkspaceVmStatusStrip", () => {
  it("names the wait without laying its reason out in the row", () => {
    const queued = props("queued_for_capacity");
    render(
      <WorkspaceVmStatusStrip
        {...queued}
        vm={{
          status: "ok",
          data: {
            ...vmData,
            status: "queued_for_capacity",
            lastError: { reason: "host_capacity_refused", host_reason: "disk" },
          },
        }}
      />,
    );
    expect(screen.getByText("VM: Waiting for capacity")).toBeVisible();
    // The reason reaches the owner as a pop-up and in the state banner above,
    // not as text that reflows the controls (`g5.ui.capacity-wait-notice`).
    expect(screen.queryByTestId("workspace-vm-capacity-reason")).toBeNull();
    expect(screen.queryByText(/enough free disk space/)).toBeNull();
  });

  it("keeps owner and VM status while removing image, flush, loss warning, and Changes", () => {
    render(<WorkspaceVmStatusStrip {...props()} />);

    expect(screen.getByText(/Owner: Zhengchun Liu/)).toBeVisible();
    expect(screen.getByText("VM: Stopped")).toBeVisible();
    expect(
      screen.queryAllByText(/Image:|Last flush:|Unflushed changes/),
    ).toHaveLength(0);
    expect(screen.queryByRole("button", { name: "Changes" })).toBeNull();
  });

  // The terminal control stays for the owner in every state: enabled while
  // running, otherwise disabled with the reason (`g5.workspace.ui.unavailable`).
  it.each([
    ["stopped", true, false, false],
    ["failed", true, false, false],
    ["starting", false, true, false],
    ["queued_for_capacity", false, true, false],
    ["running", false, true, true],
    ["stopping", false, false, false],
  ] as const)(
    "preserves owner controls when VM is %s",
    (status, start, stop, terminalEnabled) => {
      render(<WorkspaceVmStatusStrip {...props(status)} />);

      expect(!!screen.queryByTestId("workspace-vm-start")).toBe(start);
      expect(!!screen.queryByTestId("workspace-vm-stop")).toBe(stop);
      const terminal = screen.getByTestId("workspace-terminal-open");
      if (terminalEnabled) {
        expect(terminal).toBeEnabled();
        expect(terminal).toHaveAttribute("title", "Terminal");
      } else {
        // The reason names the state (`g5.workspace.ui.vm-state`).
        expect(terminal).toBeDisabled();
        expect(terminal.getAttribute("title")).toBe(
          liveGateReason(
            liveGateKind({
              isOwner: true,
              isBase: false,
              readOnly: false,
              freezing: false,
              vmStatus: status,
            }),
            "open a terminal",
          ),
        );
        if (status === "stopped") {
          expect(terminal.getAttribute("title")).toBe(
            "Start the workspace to open a terminal.",
          );
        }
        // Trying the disabled control reveals the same reason
        // (`g5.ui.unavailable-actions`).
        fireEvent.click(
          screen.getByTestId("workspace-terminal-start-hint-trigger"),
        );
        expect(
          screen.getByTestId("workspace-terminal-start-hint"),
        ).toHaveTextContent(terminal.getAttribute("title") ?? "");
      }
      expect(screen.queryByTestId("workspace-vm-restart")).toBeNull();
    },
  );

  it.each<AsyncState<WorkspaceVmView>>([
    { status: "loading" },
    { status: "error", error: new Error("Unavailable") },
  ])("hides VM actions when the query is $status", (vm) => {
    render(<WorkspaceVmStatusStrip {...props()} vm={vm} />);
    for (const action of ["start", "stop", "restart"]) {
      expect(screen.queryByTestId(`workspace-vm-${action}`)).toBeNull();
    }
    expect(screen.getByTestId("workspace-terminal-open")).toBeDisabled();
  });

  it.each([false, true])(
    "hides the VM state and controls from a non-owner with transfer permission %s",
    (canTransfer) => {
      render(
        <WorkspaceVmStatusStrip
          {...props("running")}
          vm={{
            status: "ok",
            data: {
              ...vmData,
              status: "running",
              pendingStopDeadlineAt: "2026-09-24T03:00:00Z",
            },
          }}
          canControl={false}
          canTransfer={canTransfer}
        />,
      );
      expect(screen.getByText(/Owner: Zhengchun Liu/)).toBeVisible();
      expect(screen.queryByText(/VM:/)).toBeNull();
      expect(screen.queryByText(/Stop requested/)).toBeNull();
      for (const action of ["start", "stop", "restart"]) {
        expect(screen.queryByTestId(`workspace-vm-${action}`)).toBeNull();
      }
      expect(screen.queryByTestId("workspace-terminal-open")).toBeNull();
      expect(!!screen.queryByTestId("workspace-owner-transfer")).toBe(
        canTransfer,
      );
    },
  );

  it.each(["running", "starting", "stopped"] as const)(
    "shows the %s VM label only to the owner",
    (status) => {
      const { rerender } = render(
        <WorkspaceVmStatusStrip {...props(status)} canControl={false} />,
      );
      expect(screen.queryByText(/VM:/)).toBeNull();
      rerender(<WorkspaceVmStatusStrip {...props(status)} />);
      expect(screen.getByText(/VM:/)).toBeVisible();
    },
  );

  it.each(["running", "stopped"] as const)(
    "omits Base owner, VM state and actions even with a cached %s VM",
    (status) => {
      render(<WorkspaceVmStatusStrip {...props(status)} isBase />);
      expect(screen.queryByTestId("workspace-vm-status-strip")).toBeNull();
      expect(screen.queryByText(/Owner:|VM:/)).toBeNull();
      expect(screen.queryByTestId("workspace-fork-terminal")).toBeNull();
    },
  );

  it("preserves callbacks, busy states, and member filtering as the VM changes state", async () => {
    const user = userEvent.setup();
    const handlers = props();
    const { rerender } = render(<WorkspaceVmStatusStrip {...handlers} busy />);
    expect(screen.getByTestId("workspace-vm-start")).toBeDisabled();
    expect(screen.getByTestId("workspace-owner-transfer")).toBeDisabled();
    expect(screen.queryByRole("option", { name: "Zhengchun Liu" })).toBeNull();
    await user.selectOptions(screen.getByRole("combobox"), "member");
    expect(screen.getByTestId("workspace-owner-transfer")).toBeDisabled();
    rerender(<WorkspaceVmStatusStrip {...handlers} />);
    await user.click(screen.getByTestId("workspace-owner-transfer"));
    expect(handlers.onTransfer).toHaveBeenCalledWith("member");
    await user.click(screen.getByTestId("workspace-vm-start"));
    expect(handlers.onResume).toHaveBeenCalledOnce();

    const running: AsyncState<WorkspaceVmView> = {
      status: "ok",
      data: { ...vmData, status: "running" },
    };
    rerender(<WorkspaceVmStatusStrip {...handlers} vm={running} busy />);
    expect(screen.queryByTestId("workspace-vm-start")).toBeNull();
    expect(screen.getByTestId("workspace-vm-stop")).toBeDisabled();
    expect(screen.queryByTestId("workspace-vm-restart")).toBeNull();
    const terminalButton = screen.getByRole("button", { name: "Terminal" });
    expect(terminalButton.textContent).toBe("");
    expect(terminalButton).toHaveAttribute("title", "Terminal");
    await user.click(terminalButton);
    expect(handlers.onOpenTerminal).toHaveBeenCalledOnce();
    rerender(<WorkspaceVmStatusStrip {...handlers} vm={running} />);
    await user.click(screen.getByTestId("workspace-vm-stop"));
    expect(handlers.onStop).toHaveBeenCalledOnce();
  });

  it("retains the pending stop deadline and action errors", () => {
    render(
      <WorkspaceVmStatusStrip
        {...props()}
        vm={{
          status: "ok",
          data: { ...vmData, pendingStopDeadlineAt: "2026-09-24T03:00:00Z" },
        }}
        error="Could not stop workspace"
      />,
    );
    expect(screen.getByText(/Stop requested · deadline/)).toBeVisible();
    expect(screen.getByText("Could not stop workspace")).toBeVisible();
  });
});
