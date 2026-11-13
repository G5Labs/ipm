// =============================================================================
// G5 Workspace Page — NotificationPanel
// =============================================================================
// Slide-out panel listing the unread notifications (the same set the bell
// counts), grouped by type, with action buttons and a "Clear all" option
// that marks them read (g5.user.notifications).
//
// Accessible by clicking the notification bell icon in the TopBar.
// Slides in from the right edge of the viewport.
// Closes on outside click, Escape key, or clicking the bell again.
// =============================================================================

import React, { useEffect, useRef, useCallback, useMemo } from "react";
import {
  X,
  Bell,
  Check,
  Zap,
  AtSign,
  MessageSquare,
  ClipboardCheck,
} from "lucide-react";
import { classNames, Button, SectionTitle } from "../ui/UI";
import type { G5Notification, NotificationAction } from "../../lib/types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface NotificationPanelProps {
  notifications: G5Notification[];
  onAction: (
    id: string,
    action: NotificationAction,
    notification?: G5Notification,
  ) => void | Promise<void>;
  onClearAll: () => void;
  onClose: () => void;
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface NotificationGroup {
  id: string;
  label: string;
  icon: React.ReactNode;
  color: string;
  items: G5Notification[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Get notification type group label */
function getGroupId(notification: G5Notification): string {
  if (notification.category === "mention") {
    return "mentions";
  }
  // Review activity (rounds pushed, decisions, a source moving on): its
  // own group, not a green "Completed" tick.
  if (notification.category === "review") {
    return "review";
  }
  if (
    notification.category === "comment" ||
    notification.commentId ||
    notification.replyId
  ) {
    return "comments";
  }

  switch (notification.category) {
    case "task_blocked":
    case "agent_error":
      return "needs_attention";
    case "build_failure":
    case "test_failure":
      return "completed";
    case "spend_warning":
    case "spend_limit":
      return "needs_attention";
    case "worker_terminated":
      return "completed";
    case "merge_conflict":
      return "needs_attention";
    default:
      return "completed";
  }
}

/** Format relative time */
function relativeTime(isoString: string): string {
  const diff = Date.now() - new Date(isoString).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/** Group notifications by type */
function groupNotifications(
  notifications: G5Notification[],
): NotificationGroup[] {
  const groups: Record<string, G5Notification[]> = {
    completed: [],
    needs_attention: [],
    mentions: [],
    comments: [],
    review: [],
  };

  notifications.forEach((n) => {
    const groupId = getGroupId(n);
    if (groups[groupId]) {
      groups[groupId].push(n);
    } else {
      groups.completed.push(n);
    }
  });

  const result: NotificationGroup[] = [];

  if (groups.needs_attention.length > 0) {
    result.push({
      id: "needs_attention",
      label: "Needs Attention",
      icon: <Zap size={14} />,
      color: "var(--g5-warn)",
      items: groups.needs_attention,
    });
  }

  if (groups.mentions.length > 0) {
    result.push({
      id: "mentions",
      label: "Mentions",
      icon: <AtSign size={14} />,
      color: "var(--g5-info)",
      items: groups.mentions,
    });
  }

  if (groups.review.length > 0) {
    result.push({
      id: "review",
      label: "Review",
      icon: <ClipboardCheck size={14} />,
      color: "var(--g5-info)",
      items: groups.review,
    });
  }

  if (groups.comments.length > 0) {
    result.push({
      id: "comments",
      label: "Comments",
      icon: <MessageSquare size={14} />,
      color: "var(--g5-info)",
      items: groups.comments,
    });
  }

  if (groups.completed.length > 0) {
    result.push({
      id: "completed",
      label: "Completed",
      icon: <Check size={14} />,
      color: "var(--g5-ok)",
      items: groups.completed,
    });
  }

  return result;
}

// ---------------------------------------------------------------------------
// Single Notification Item
// ---------------------------------------------------------------------------

interface NotificationItemViewProps {
  notification: G5Notification;
  onAction: (
    id: string,
    action: NotificationAction,
    notification?: G5Notification,
  ) => void | Promise<void>;
}

const NotificationItemView: React.FC<NotificationItemViewProps> = ({
  notification,
  onAction,
}) => {
  // Determine the primary action button based on category
  const primaryAction =
    notification.actions?.[0] ??
    (notification.commentId
      ? {
          label: notification.replyId ? "View Thread" : "View Comment",
          action: "navigate" as const,
          target: `comment:${notification.commentId}`,
        }
      : undefined);

  return (
    <div
      className={classNames(
        "flex items-start gap-3 px-4 py-3",
        "hover:bg-[var(--g5-bg2)]",
        "transition-colors duration-100",
      )}
    >
      {/* Unread indicator */}
      <div className="shrink-0 mt-2 w-2 h-2">
        {!notification.readAt && (
          <div
            className="w-2 h-2 rounded-full"
            style={{ backgroundColor: "var(--g5-info)" }}
            aria-label="Unread"
          />
        )}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-[var(--g5-text0)]">
          {notification.title}
        </div>
        <div className="text-xs text-[var(--g5-text1)] mt-0.5 line-clamp-2">
          {notification.message}
        </div>
        <div className="text-[10px] text-[var(--g5-text2)] mt-1">
          {relativeTime(notification.createdAt)}
        </div>

        {/* Action buttons */}
        {primaryAction && (
          <div className="mt-2">
            <Button
              kind="ghost"
              data-testid="notification-panel-action"
              onClick={() =>
                void onAction(notification.id, primaryAction, notification)
              }
            >
              {primaryAction.label}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// NotificationPanel Component
// ---------------------------------------------------------------------------

export const NotificationPanel: React.FC<NotificationPanelProps> = ({
  notifications,
  onAction,
  onClearAll,
  onClose,
}) => {
  const panelRef = useRef<HTMLDivElement>(null);

  const unreadCount = notifications.length;

  // Group notifications
  const groups = useMemo(
    () => groupNotifications(notifications),
    [notifications],
  );

  // ---- Close on Escape ----
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  // ---- Close on outside click ----
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    // Use setTimeout to avoid closing immediately on the same click that opened
    const timer = setTimeout(() => {
      document.addEventListener("mousedown", handleClick);
    }, 100);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("mousedown", handleClick);
    };
  }, [onClose]);

  return (
    <div
      ref={panelRef}
      className={classNames(
        // Anchored under the 44px title bar, inset by the shell frame when a
        // theme draws one (--shell-frame-inset, 0 by default).
        "fixed top-[calc(44px+var(--shell-frame-inset))] right-[var(--shell-frame-inset)] z-40",
        "w-[360px] h-[calc(100vh-44px-2*var(--shell-frame-inset))]",
        "bg-[var(--g5-bg1)]",
        "border-l border-[var(--g5-stroke)]",
        "flex flex-col",
        "shadow-[var(--shadow-pane)]",
        // Slide-in animation
        "animate-[slideInRight_200ms_ease-out]",
      )}
      role="dialog"
      aria-label="Notifications"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--g5-stroke)]">
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-[var(--g5-text0)]">
            Notifications
          </span>
          {unreadCount > 0 && (
            <span
              className={classNames(
                "inline-flex items-center justify-center",
                "min-w-[20px] h-5 px-1.5",
                "text-[10px] font-bold",
                "rounded-full",
                "bg-[var(--g5-info)] text-white",
              )}
            >
              {unreadCount}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {notifications.length > 0 && (
            <button
              type="button"
              data-testid="notification-panel-clear-all"
              className="text-xs text-[var(--g5-info)] hover:underline cursor-pointer"
              onClick={onClearAll}
            >
              Clear all
            </button>
          )}
          <button
            type="button"
            className={classNames(
              "p-1",
              "text-[var(--g5-text2)]",
              "hover:text-[var(--g5-text0)]",
              "transition-colors duration-150",
              "cursor-pointer",
            )}
            onClick={onClose}
            aria-label="Close notifications"
            data-testid="notification-panel-close"
          >
            <X size={16} />
          </button>
        </div>
      </div>

      {/* Notification List */}
      <div className="flex-1 overflow-y-auto">
        {notifications.length === 0 ? (
          /* Empty state */
          <div className="flex flex-col items-center justify-center h-full text-[var(--g5-text2)]">
            <Bell size={40} className="mb-4 opacity-30" />
            <p className="text-sm font-medium">No notifications</p>
            <p className="text-xs mt-1">You&apos;re all caught up!</p>
          </div>
        ) : (
          /* Grouped notifications */
          groups.map((group) => (
            <div key={group.id} data-testid={`notification-group-${group.id}`}>
              {/* Group header */}
              <div className="flex items-center gap-2 px-4 pt-4 pb-2">
                <span style={{ color: group.color }}>{group.icon}</span>
                <SectionTitle>{group.label}</SectionTitle>
                <span className="text-[10px] text-[var(--g5-text2)]">
                  ({group.items.length})
                </span>
              </div>

              {/* Group items */}
              {group.items.map((notification) => (
                <NotificationItemView
                  key={notification.id}
                  notification={notification}
                  onAction={onAction}
                />
              ))}
            </div>
          ))
        )}
      </div>

      {/* Inline keyframe animation for slide-in */}
      <style>{`
        @keyframes slideInRight {
          from {
            transform: translateX(100%);
          }
          to {
            transform: translateX(0);
          }
        }
      `}</style>
    </div>
  );
};
