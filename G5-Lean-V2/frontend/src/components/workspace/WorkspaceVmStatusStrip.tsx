import { Play, Square, SquareTerminal, UserRound } from "lucide-react";
import { useMemo, useState } from "react";
import type {
  AsyncState,
  OrgMemberDTO,
  WorkspaceVmView,
} from "../../lib/types";
import { GatedControl } from "../../lib/workspaceLiveGate";
import { classNames } from "../ui/UI";
import {
  memberLabel,
  ownerName,
  transferCandidates,
} from "../../lib/workspaceOwner";
import {
  VM_STATUS_LABEL,
  terminalControlState,
  vmControlAdmission,
  vmControlStatus,
} from "../../lib/workspaceVmControls";

interface WorkspaceVmStatusStripProps {
  vm: AsyncState<WorkspaceVmView>;
  ownerUserId: string | null;
  members: OrgMemberDTO[];
  canControl: boolean;
  canTransfer: boolean;
  isBase: boolean;
  busy: boolean;
  error?: string | null;
  onResume: () => void;
  onStop: () => void;
  onOpenTerminal: () => void;
  onTransfer: (userId: string) => void;
  /** The mobile layout has no center tabs, so it offers no Terminal. */
  showTerminal?: boolean;
}

export function WorkspaceVmStatusStrip({
  vm,
  ownerUserId,
  members,
  canControl,
  canTransfer,
  isBase,
  busy,
  error,
  onResume,
  onStop,
  onOpenTerminal,
  onTransfer,
  showTerminal: terminalOffered = true,
}: WorkspaceVmStatusStripProps) {
  const [transferTarget, setTransferTarget] = useState("");
  const ownerLabel = useMemo(
    () => ownerName(members, ownerUserId),
    [members, ownerUserId],
  );
  const data = vm.status === "ok" ? vm.data : null;
  const status = vmControlStatus(vm);
  const { canStart, canStop } = vmControlAdmission(status, {
    canControl,
    isBase,
  });
  // The terminal needs the running VM; the owner keeps the control while
  // the VM is not running, disabled with the reason
  // (`g5.workspace.ui.unavailable`).
  const showTerminal = canControl && !isBase && terminalOffered;
  const terminal = terminalControlState(vm);
  const canOpenTerminal = showTerminal && terminal.enabled;

  if (isBase) return null;

  return (
    <div data-testid="workspace-vm-status-strip" className="contents text-xs">
      <span className="flex items-center gap-1 whitespace-nowrap text-[var(--g5-text1)]">
        <UserRound className="h-3.5 w-3.5" />
        Owner: {ownerLabel}
      </span>
      {canTransfer && !isBase && (
        <div className="flex items-center gap-1">
          <select
            aria-label="Transfer workspace owner"
            value={transferTarget}
            onChange={(event) => setTransferTarget(event.target.value)}
            className="w-[7.5rem] rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-[var(--g5-text0)]"
          >
            <option value="">Transfer to…</option>
            {transferCandidates(members, ownerUserId).map((member) => (
              <option key={member.userId} value={member.userId}>
                {memberLabel(member)}
              </option>
            ))}
          </select>
          <button
            type="button"
            data-testid="workspace-owner-transfer"
            disabled={!transferTarget || busy}
            onClick={() => onTransfer(transferTarget)}
            className="rounded px-2 py-1 text-[var(--g5-active)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
          >
            Transfer
          </button>
        </div>
      )}
      {/* Like its controls, the VM's state is shown only to its owner. A
        capacity wait's reason is not laid out here: the pop-up and the state
        banner directly above carry it (`g5.ui.capacity-wait-notice`). */}
      {canControl && (
        <span
          className={classNames(
            "whitespace-nowrap rounded-full px-2 py-0.5 font-medium",
            status === "running"
              ? "bg-[var(--g5-ok)]/15 text-[var(--g5-ok)]"
              : status === "failed"
                ? "bg-[var(--g5-blocked)]/15 text-[var(--g5-blocked)]"
                : "bg-[var(--g5-bg2)] text-[var(--g5-text1)]",
          )}
        >
          VM: {VM_STATUS_LABEL[status] ?? status}
        </span>
      )}
      {canControl && data?.pendingStopDeadlineAt && (
        <span className="text-[var(--g5-warn)]">
          Stop requested · deadline{" "}
          {new Date(data.pendingStopDeadlineAt).toLocaleTimeString()}
        </span>
      )}
      {error && <span className="text-[var(--g5-blocked)]">{error}</span>}
      {showTerminal && (
        // Trying the disabled control reveals the reason and Start
        // (`g5.ui.unavailable-actions`).
        <GatedControl
          verb="open a terminal"
          policyFree
          reason={canOpenTerminal ? null : terminal.title}
          testId="workspace-terminal-start-hint"
        >
          <button
            type="button"
            data-testid="workspace-terminal-open"
            data-workspace-activity="ignore"
            aria-label="Terminal"
            title={terminal.title}
            disabled={!canOpenTerminal}
            onClick={onOpenTerminal}
            className="flex items-center rounded px-2 py-1 text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:cursor-not-allowed disabled:opacity-50"
          >
            <SquareTerminal aria-hidden="true" className="h-3.5 w-3.5" />
          </button>
        </GatedControl>
      )}
      {canStart && (
        <button
          type="button"
          data-testid="workspace-vm-start"
          aria-label="Start workspace"
          onClick={onResume}
          disabled={busy}
          className="flex items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-[var(--g5-active)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
        >
          <Play className="h-3.5 w-3.5" /> Start
        </button>
      )}
      {canStop && (
        <button
          type="button"
          data-testid="workspace-vm-stop"
          onClick={onStop}
          disabled={busy}
          className="flex items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
        >
          <Square className="h-3.5 w-3.5" /> Stop
        </button>
      )}
    </div>
  );
}
