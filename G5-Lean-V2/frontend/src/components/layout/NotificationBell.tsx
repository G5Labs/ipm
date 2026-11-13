// =============================================================================
// G5 Client — Notification bell
// =============================================================================
// The bell with its unread badge, shared by the classic title bar and the
// alignment shell's strip controls (workbench g5.workbench.shell-chrome).
// =============================================================================

import { Bell } from "lucide-react";
import React from "react";
import { classNames } from "../ui/UI";

export const NotificationBell: React.FC<{
  unreadCount: number;
  onClick: () => void;
  /** Smaller control for the tab strip; default is the title bar's 32px. */
  size?: "sm" | "md";
}> = ({ unreadCount, onClick, size = "md" }) => {
  return (
    <button
      type="button"
      className={classNames(
        "relative inline-flex items-center justify-center",
        size === "sm" ? "w-7 h-7" : "w-8 h-8",
        "rounded-[var(--g5-radius-sm)]",
        "text-[var(--g5-text1)]",
        "hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
        "transition-colors duration-150",
        "cursor-pointer",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
      )}
      onClick={onClick}
      data-testid="top-bar-notifications"
      title="Notifications"
      aria-label="Notifications"
    >
      <Bell size={size === "sm" ? 15 : 16} />
      {unreadCount > 0 && (
        <span
          className={classNames(
            "absolute -top-1 -right-1",
            "w-4 h-4 rounded-full",
            "bg-[var(--g5-blocked)] text-white",
            "text-[10px] font-medium",
            "flex items-center justify-center",
          )}
        >
          {unreadCount > 99 ? "99+" : unreadCount}
        </span>
      )}
    </button>
  );
};
