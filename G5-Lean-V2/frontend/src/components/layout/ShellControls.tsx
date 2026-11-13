// =============================================================================
// G5 Client — Shell controls
// =============================================================================
// The global controls at the right end of the workspace tab strip under the
// alignment layout (workbench g5.workbench.shell-chrome): Workers,
// Notifications, Report a problem, Settings and Account. Each page supplies
// the handlers it has; an omitted handler renders no control. The test ids
// are the title bar's, so flows that reach these surfaces keep working.
// =============================================================================

import { CircleUserRound, Server, Settings } from "lucide-react";
import React from "react";
import { PlatformIssueReportTrigger } from "../platform-issues/PlatformIssueReportButton";
import { classNames } from "../ui/UI";
import { NotificationBell } from "./NotificationBell";

export interface ShellControlsProps {
  /** Open the org worker-host list (people who manage workers). */
  onOpenWorkerHosts?: () => void;
  /** Notifications flyout with its unread count. */
  notifications?: { unreadCount: number; onOpen: () => void };
  /** Open the settings page. */
  onOpenSettings?: () => void;
  /** Open the Account / organizations flyout. */
  onOpenAccount?: () => void;
}

const ICON_BUTTON = classNames(
  "inline-flex h-7 w-7 items-center justify-center rounded-[var(--g5-radius-sm)]",
  "text-[var(--g5-text1)] transition-colors duration-150 cursor-pointer",
  "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
);

export const ShellControls: React.FC<ShellControlsProps> = ({
  onOpenWorkerHosts,
  notifications,
  onOpenSettings,
  onOpenAccount,
}) => (
  <div
    data-testid="shell-controls"
    role="group"
    aria-label="Global controls"
    className="ml-auto flex shrink-0 items-center gap-0.5 self-center pl-2"
  >
    {onOpenWorkerHosts && (
      <button
        type="button"
        data-testid="top-bar-worker-hosts"
        className={ICON_BUTTON}
        onClick={onOpenWorkerHosts}
        title="Workers"
        aria-label="Workers"
      >
        <Server size={15} />
      </button>
    )}
    {notifications && (
      <NotificationBell
        size="sm"
        unreadCount={notifications.unreadCount}
        onClick={notifications.onOpen}
      />
    )}
    <PlatformIssueReportTrigger placement="topbar" />
    {onOpenSettings && (
      <button
        type="button"
        data-testid="settings-button"
        className={ICON_BUTTON}
        onClick={onOpenSettings}
        title="Settings"
        aria-label="Settings"
      >
        <Settings size={15} />
      </button>
    )}
    {onOpenAccount && (
      <button
        type="button"
        data-testid="org-management-button"
        className={ICON_BUTTON}
        onClick={onOpenAccount}
        title="Account and organizations"
        aria-label="Account and organizations"
      >
        <CircleUserRound size={15} />
      </button>
    )}
  </div>
);
