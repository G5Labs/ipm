// =============================================================================
// G5 Client — Workspace VM control admission
// =============================================================================
// One rule for the owner's lifecycle control, read by the phone lifecycle
// control's `WorkspaceVmStatusStrip` and the ribbon's
// `WorkbenchWorkspaceControls` (workspaces `g5.workspace.ui.gates`, "Name
// each VM state"): which state admits Start, which admits Stop, and what the
// one button says in each state.
// =============================================================================

import type { AsyncState, WorkspaceVmStatus, WorkspaceVmView } from "./types";
import {
  liveGateCanStart,
  liveGateKind,
  liveGateReason,
} from "./workspaceLiveGate";

/** The VM's status, or why the page has none yet. */
export type VmControlStatus = WorkspaceVmStatus | "loading" | "unavailable";

/** The state name shown beside or over the control. */
export const VM_STATUS_LABEL: Record<string, string> = {
  stopped: "Stopped",
  starting: "Starting",
  queued_for_capacity: "Waiting for capacity",
  running: "Running",
  stopping: "Stopping",
  failed: "Failed",
  loading: "Loading",
  unavailable: "Unavailable",
};

/** The VM status the page shows, from the async VM view. */
export function vmControlStatus(
  vm: AsyncState<WorkspaceVmView>,
): VmControlStatus {
  if (vm.status === "ok") return vm.data.status;
  return vm.status === "loading" ? "loading" : "unavailable";
}

/** The owner's one valid lifecycle action in this state, if any. */
export function vmControlAdmission(
  status: VmControlStatus,
  options: { canControl: boolean; isBase: boolean },
): { canStart: boolean; canStop: boolean } {
  const eligible = options.canControl && !options.isBase;
  return {
    // The same predicate as the live gate's Start (`liveGateCanStart`).
    canStart:
      eligible &&
      (status === "stopped" || status === "failed") &&
      liveGateCanStart(status),
    canStop:
      eligible &&
      (status === "running" ||
        status === "starting" ||
        status === "queued_for_capacity"),
  };
}

export interface VmButtonFace {
  /** Which action the button performs; `null` while the VM transitions. */
  action: "start" | "stop" | null;
  /** The label on the button. */
  label: string;
}

/**
 * The one-button face of the lifecycle control (g5.workbench.shell-chrome):
 * Start while stopped or failed, Stop while running, a transitional label
 * with no action while starting, waiting for capacity or stopping.
 */
export function vmButtonFace(status: VmControlStatus): VmButtonFace {
  switch (status) {
    case "running":
      return { action: "stop", label: "Stop" };
    case "starting":
      return { action: null, label: "Starting…" };
    case "queued_for_capacity":
      return { action: null, label: "Waiting…" };
    case "stopping":
      return { action: null, label: "Stopping…" };
    default:
      return { action: "start", label: "Start" };
  }
}

/**
 * The owner's Terminal control: the terminal needs the running VM; while it
 * is not running the control stays visible, disabled with the reason
 * (`g5.workspace.ui.unavailable`). Shared by the classic strip and the
 * alignment rail's Files header (g5.workbench.shell-chrome).
 */
export function terminalControlState(vm: AsyncState<WorkspaceVmView>): {
  enabled: boolean;
  title: string;
} {
  const vmStatus = vm.status === "ok" ? vm.data.status : null;
  const enabled = vmStatus === "running";
  return {
    enabled,
    title: enabled
      ? "Terminal"
      : (liveGateReason(
          liveGateKind({
            isOwner: true,
            isBase: false,
            readOnly: false,
            freezing: false,
            vmStatus,
          }),
          "open a terminal",
        ) ?? "Terminal"),
  };
}
