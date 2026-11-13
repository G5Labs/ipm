import { describe, expect, it } from "vitest";

import { createMockWorkspaceVm } from "../test/fixtures";
import {
  terminalControlState,
  vmButtonFace,
  vmControlAdmission,
  vmControlStatus,
} from "./workspaceVmControls";

// One admission rule for both VM controls (workspaces g5.workspace.ui.gates).
describe("workspaceVmControls", () => {
  it("admits Start from stopped or failed and Stop while running, starting or queued, for the owner of a non-Base workspace", () => {
    const owner = { canControl: true, isBase: false };
    expect(vmControlAdmission("stopped", owner)).toEqual({
      canStart: true,
      canStop: false,
    });
    expect(vmControlAdmission("failed", owner)).toEqual({
      canStart: true,
      canStop: false,
    });
    for (const status of [
      "running",
      "starting",
      "queued_for_capacity",
    ] as const) {
      expect(vmControlAdmission(status, owner)).toEqual({
        canStart: false,
        canStop: true,
      });
    }
    for (const status of ["stopping", "loading", "unavailable"] as const) {
      expect(vmControlAdmission(status, owner)).toEqual({
        canStart: false,
        canStop: false,
      });
    }
    expect(
      vmControlAdmission("stopped", { canControl: false, isBase: false }),
    ).toEqual({ canStart: false, canStop: false });
    expect(
      vmControlAdmission("running", { canControl: true, isBase: true }),
    ).toEqual({ canStart: false, canStop: false });
  });

  it("names the one-button face per status", () => {
    expect(vmButtonFace("stopped")).toEqual({
      action: "start",
      label: "Start",
    });
    expect(vmButtonFace("failed")).toEqual({ action: "start", label: "Start" });
    expect(vmButtonFace("running")).toEqual({ action: "stop", label: "Stop" });
    expect(vmButtonFace("starting")).toEqual({
      action: null,
      label: "Starting…",
    });
    expect(vmButtonFace("queued_for_capacity")).toEqual({
      action: null,
      label: "Waiting…",
    });
    expect(vmButtonFace("stopping")).toEqual({
      action: null,
      label: "Stopping…",
    });
    expect(vmButtonFace("unavailable")).toEqual({
      action: "start",
      label: "Start",
    });
  });

  it("reads the status from the async view", () => {
    expect(vmControlStatus({ status: "loading" })).toBe("loading");
    expect(vmControlStatus({ status: "error", error: new Error("x") })).toBe(
      "unavailable",
    );
  });

  it("enables the Terminal only while the VM runs and otherwise names the reason", () => {
    expect(
      terminalControlState({
        status: "ok",
        data: createMockWorkspaceVm({ status: "running" }),
      }),
    ).toEqual({ enabled: true, title: "Terminal" });
    const stopped = terminalControlState({
      status: "ok",
      data: createMockWorkspaceVm({ status: "stopped" }),
    });
    expect(stopped.enabled).toBe(false);
    expect(stopped.title).not.toBe("Terminal");
    expect(terminalControlState({ status: "loading" }).enabled).toBe(false);
  });
});
