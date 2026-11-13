// =============================================================================
// G5 Workbench — Workspace controls at the ribbon's right end
// =============================================================================
// The workspace-bound controls the title bar used to carry, under the
// alignment layout (workbench g5.workbench.shell-chrome): the owner as a
// user-with-key button that opens "Transfer ownership" for people who may
// transfer; one VM button that says Start or Stop by the VM status and only
// names a transition while one runs; Workspace details; and the Chat toggle
// while the center tab strip is hidden. Every button is a `RibbonShapedButton`
// in the person's action-bar style, so the row reads as one bar. Admission
// follows `lib/workspaceVmControls.ts`, the classic strip's rule.
// =============================================================================

import { Info, Loader2, MessageSquare, Play, Square } from "lucide-react";
import React, { useCallback, useMemo, useRef, useState } from "react";
import { useDismissOnOutside } from "../../hooks/useDismissOnOutside";
import type {
  AsyncState,
  OrgMemberDTO,
  RibbonStyle,
  WorkspaceVmView,
} from "../../lib/types";
import {
  VM_STATUS_LABEL,
  vmButtonFace,
  vmControlAdmission,
  vmControlStatus,
} from "../../lib/workspaceVmControls";
import { describeCapacityWait } from "../workspace/vmCapacityWait";
import {
  memberLabel,
  ownerName,
  transferCandidates,
} from "../../lib/workspaceOwner";
import { Button, classNames } from "../ui/UI";
import {
  RIBBON_MENU_CLASS,
  RIBBON_MENU_ITEM_CLASS,
  RibbonShapedButton,
  ribbonButtonShape,
} from "./ActionRibbon";

export interface WorkbenchWorkspaceControlsProps {
  /**
   * The ribbon's rendered style (the person's "Action bar" setting, resolved
   * by the dynamic density); the buttons take the same shape.
   */
  variant?: RibbonStyle;
  vm: AsyncState<WorkspaceVmView>;
  ownerUserId: string | null;
  members: OrgMemberDTO[];
  /** The viewer owns the workspace: VM state and lifecycle control shown. */
  canControl: boolean;
  /** The viewer may transfer ownership (owner, org admin). */
  canTransfer: boolean;
  /** Base: no owner row, no VM; Workspace details only. */
  isBase: boolean;
  /** A lifecycle or transfer call is in flight. */
  busy: boolean;
  /** The last lifecycle or transfer error, shown beside the controls. */
  error?: string | null;
  onResume: () => void;
  onStop: () => void;
  onTransfer: (userId: string) => void;
  onOpenDetails?: () => void;
  /** Rendered only while the center tab strip is hidden and chat exists. */
  chatToggle?: { open: boolean; onToggle: () => void };
}

/** A user silhouette with a key badge: the owner (the reviewed mock's icon). */
export const OwnerKeyIcon: React.FC<{ size?: number; className?: string }> = ({
  size = 22,
  className,
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    aria-hidden="true"
    data-testid="owner-key-icon"
  >
    <circle cx="10" cy="7" r="4" />
    <path d="M2.5 21a7.5 7.5 0 0 1 12.6-5.5" />
    <circle cx="17.5" cy="17.5" r="4.5" fill="currentColor" stroke="none" />
    <g stroke="var(--g5-bg1)" strokeWidth="1.4" fill="none">
      <circle cx="16.3" cy="16.3" r="1.4" />
      <path d="m17.3 17.3 2.6 2.6" />
      <path d="m19 19 .9-.9" />
    </g>
  </svg>
);

export const WorkbenchWorkspaceControls: React.FC<
  WorkbenchWorkspaceControlsProps
> = ({
  variant = "extended-icons",
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
  onTransfer,
  onOpenDetails,
  chatToggle,
}) => {
  const shape = ribbonButtonShape(variant, {});
  const iconSize = shape.iconSize;
  const [menuOpen, setMenuOpen] = useState(false);
  const [pick, setPick] = useState<OrgMemberDTO | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);

  const others = useMemo(
    () => transferCandidates(members, ownerUserId),
    [members, ownerUserId],
  );

  // The menu closes on an outside click or Escape, like the ribbon's menus.
  const closeMenu = useCallback(() => {
    setMenuOpen(false);
    setPick(null);
    // A keyboard user dismissing from inside the menu lands back on the
    // trigger instead of on the document body.
    const root = rootRef.current;
    if (root && root.contains(document.activeElement)) {
      root.querySelector<HTMLButtonElement>("button")?.focus();
    }
  }, []);
  useDismissOnOutside(rootRef, menuOpen, closeMenu);

  const data = vm.status === "ok" ? vm.data : null;
  const status = vmControlStatus(vm);
  const { canStart, canStop } = vmControlAdmission(status, {
    canControl,
    isBase,
  });
  const face = vmButtonFace(status);
  const statusLabel = VM_STATUS_LABEL[status] ?? status;
  const capacityReason =
    status === "queued_for_capacity"
      ? describeCapacityWait(data?.lastError)
      : null;
  const stopDeadline = data?.pendingStopDeadlineAt
    ? `Stop requested · deadline ${new Date(data.pendingStopDeadlineAt).toLocaleTimeString()}`
    : null;
  // The tooltip carries the state and every reason the strip used to show.
  // The capacity reason is never laid out in the row: it is announced as a
  // pop-up and kept here and in the state banner, so the buttons do not move
  // when a wait begins or ends (`g5.ui.capacity-wait-notice`).
  const vmTitle = [`VM: ${statusLabel}`, capacityReason, stopDeadline, error]
    .filter(Boolean)
    .join(" · ");
  const owner = ownerName(members, ownerUserId);
  const ownerLabel = `Owner: ${owner}`;
  const transferEnabled = canTransfer && !isBase;
  const menuId = "workspace-owner-menu";

  return (
    <div
      data-testid="workbench-workspace-controls"
      className="flex min-w-0 flex-wrap items-start gap-0.5"
    >
      {!isBase && (
        // The dismissal boundary is the owner control alone: a click on a
        // sibling control closes the menu like any outside click.
        <div ref={rootRef} className="relative min-w-0">
          <RibbonShapedButton
            shape={shape}
            testId="workspace-owner-trigger"
            icon={<OwnerKeyIcon size={iconSize} className="shrink-0" />}
            label={ownerLabel}
            // A long name truncates on its own line under the icon; the
            // tooltip keeps it whole.
            stacked={{ head: "Owner:", tail: owner }}
            title={
              transferEnabled
                ? `${ownerLabel} · Transfer ownership`
                : ownerLabel
            }
            caret={transferEnabled}
            menuProps={
              transferEnabled ? { open: menuOpen, id: menuId } : undefined
            }
            disabled={!transferEnabled}
            className={
              transferEnabled
                ? "max-w-[12rem]"
                : "max-w-[12rem] cursor-default disabled:opacity-100"
            }
            onClick={() => {
              setMenuOpen((open) => !open);
              setPick(null);
            }}
          />
          {transferEnabled && menuOpen ? (
            <div
              id={menuId}
              role="menu"
              aria-label="Transfer ownership"
              data-testid={menuId}
              className={classNames(RIBBON_MENU_CLASS, "right-0 text-left")}
            >
              <div className="flex items-center gap-2 border-b border-[var(--g5-stroke)] px-2 pb-2 pt-1.5 text-[12px] text-[var(--g5-text1)]">
                <OwnerKeyIcon size={16} className="shrink-0" />
                <span>
                  Owner{" "}
                  <b className="font-semibold text-[var(--g5-text0)]">
                    {ownerName(members, ownerUserId)}
                  </b>
                </span>
              </div>
              <div className="px-2 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
                Transfer ownership to
              </div>
              {/* The list scrolls so the confirmation below stays in reach
                  in a large organization. */}
              <div
                data-testid="workspace-owner-options"
                className="flex max-h-56 flex-col gap-0.5 overflow-y-auto"
              >
                {others.length === 0 ? (
                  <div className="px-2 py-1.5 text-[12px] text-[var(--g5-text2)]">
                    No other members.
                  </div>
                ) : (
                  others.map((member) => (
                    <button
                      key={member.userId}
                      type="button"
                      role="menuitem"
                      data-testid="workspace-owner-option"
                      data-user-id={member.userId}
                      data-picked={
                        pick?.userId === member.userId ? "true" : undefined
                      }
                      className={RIBBON_MENU_ITEM_CLASS}
                      disabled={busy}
                      onClick={() => setPick(member)}
                    >
                      <span className="min-w-0 flex-1 truncate">
                        {memberLabel(member)}
                      </span>
                      {member.displayName ? (
                        <span className="shrink-0 text-[11px] text-[var(--g5-text2)]">
                          {member.email}
                        </span>
                      ) : null}
                    </button>
                  ))
                )}
              </div>
              {pick ? (
                <div
                  data-testid="workspace-owner-confirm"
                  className="mt-1 rounded bg-[var(--g5-bg2)] p-2 text-[12px] text-[var(--g5-text1)]"
                >
                  Transfer this workspace to{" "}
                  <b className="font-semibold text-[var(--g5-text0)]">
                    {memberLabel(pick)}
                  </b>
                  ? Its VM controls move to them.
                  <div className="mt-2 flex gap-1.5">
                    <Button
                      kind="primary"
                      data-testid="workspace-owner-transfer"
                      disabled={busy}
                      onClick={() => {
                        const target = pick.userId;
                        closeMenu();
                        onTransfer(target);
                      }}
                    >
                      Transfer
                    </Button>
                    <Button
                      kind="secondary"
                      data-testid="workspace-owner-transfer-cancel"
                      onClick={() => setPick(null)}
                    >
                      Cancel
                    </Button>
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      )}

      {/* Like its controls, the VM's state is shown only to its owner. */}
      {canControl && !isBase && (
        <RibbonShapedButton
          shape={shape}
          testId={
            face.action === "start"
              ? "workspace-vm-start"
              : face.action === "stop"
                ? "workspace-vm-stop"
                : "workspace-vm-pending"
          }
          icon={
            face.action === null ? (
              <Loader2
                size={iconSize}
                className="shrink-0 animate-spin"
                aria-hidden="true"
              />
            ) : face.action === "start" ? (
              <Play size={iconSize} className="shrink-0" aria-hidden="true" />
            ) : (
              <Square size={iconSize} className="shrink-0" aria-hidden="true" />
            )
          }
          label={face.label}
          title={vmTitle}
          disabled={
            busy ||
            (face.action === "start" && !canStart) ||
            (face.action === "stop" && !canStop) ||
            face.action === null
          }
          onClick={
            face.action === "start"
              ? onResume
              : face.action === "stop"
                ? onStop
                : undefined
          }
          style={
            face.action === "start" ? { color: "var(--g5-active)" } : undefined
          }
        />
      )}

      {onOpenDetails && (
        <RibbonShapedButton
          shape={shape}
          testId="workspace-info-button"
          icon={
            <Info size={iconSize} className="shrink-0" aria-hidden="true" />
          }
          label="Details"
          title="Workspace details"
          onClick={onOpenDetails}
        />
      )}

      {chatToggle && (
        <RibbonShapedButton
          shape={shape}
          testId="toggle-right-panel"
          icon={
            <MessageSquare
              size={iconSize}
              className="shrink-0"
              aria-hidden="true"
            />
          }
          label="Chat"
          title={chatToggle.open ? "Close chat" : "Open chat"}
          onClick={chatToggle.onToggle}
          style={chatToggle.open ? { color: "var(--g5-info)" } : undefined}
        />
      )}

      {error && (
        <span
          role="alert"
          data-testid="workspace-vm-error"
          className="max-w-[18rem] self-center text-[11px] text-[var(--g5-blocked)]"
        >
          {error}
        </span>
      )}
    </div>
  );
};
