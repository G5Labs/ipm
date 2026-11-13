// =============================================================================
// G5 Workspace Page — NotificationToastStack
// =============================================================================
// Portal-rendered stack of toast notifications in the bottom-right corner.
// Supports auto-dismiss (info), sticky behavior (warning/error), action
// buttons, and enter/exit animations.
//
// MUST render via ReactDOM.createPortal to ensure toasts appear above all
// other content regardless of parent stacking contexts.
// =============================================================================

import React, { useState, useEffect, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import { Info, AlertTriangle, XCircle, X } from "lucide-react";
import { classNames, Button } from "../ui/UI";
import type { G5Notification, NotificationAction } from "../../lib/types";
import { ACTION_TOAST_LIFETIME_MS } from "../../hooks/useClientToasts";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface NotificationToastStackProps {
  notifications: G5Notification[];
  onDismiss: (id: string) => void;
  onAction: (id: string, action: NotificationAction) => void;
}

// Re-export types for convenience
export type { NotificationAction, G5Notification };

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_VISIBLE = 5;
const AUTO_DISMISS_MS = 5_000;
const ENTER_DURATION_MS = 300;
const EXIT_DURATION_MS = 300;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Get severity icon */
function severityIcon(severity: G5Notification["severity"]): React.ReactNode {
  switch (severity) {
    case "info":
      return <Info size={18} />;
    case "warning":
      return <AlertTriangle size={18} />;
    case "error":
      return <XCircle size={18} />;
  }
}

/** Get severity accent color CSS variable */
function severityColor(severity: G5Notification["severity"]): string {
  switch (severity) {
    case "info":
      return "var(--g5-info)";
    case "warning":
      return "var(--g5-warn)";
    case "error":
      return "var(--g5-blocked)";
  }
}

/** Format relative time (e.g., "2m ago") */
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

// ---------------------------------------------------------------------------
// Single Toast Component
// ---------------------------------------------------------------------------

interface ToastProps {
  notification: G5Notification;
  onDismiss: (id: string) => void;
  onAction: (id: string, action: NotificationAction) => void;
  entering: boolean;
  exiting: boolean;
}

const Toast: React.FC<ToastProps> = ({
  notification,
  onDismiss,
  onAction,
  entering,
  exiting,
}) => {
  const color = severityColor(notification.severity);

  return (
    <div
      className={classNames(
        "relative max-w-[380px] w-[380px]",
        "bg-[var(--g5-bg2)]",
        "border border-[var(--g5-stroke)]",
        "rounded-[var(--g5-radius-md)]",
        "shadow-[var(--shadow-pane)]",
        "overflow-hidden",
      )}
      style={{
        borderLeftWidth: "3px",
        borderLeftColor: color,
        transition: `transform ${ENTER_DURATION_MS}ms ease-out, opacity ${EXIT_DURATION_MS}ms ease-out`,
        transform: entering ? "translateX(100%)" : "translateX(0)",
        opacity: exiting ? 0 : 1,
      }}
      role="alert"
      aria-live="polite"
      data-testid="toast"
    >
      <div className="flex items-start gap-3 p-3">
        {/* Severity icon */}
        <span className="shrink-0 mt-0.5" style={{ color }}>
          {severityIcon(notification.severity)}
        </span>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="text-sm font-medium text-[var(--g5-text0)]">
            {notification.title}
          </div>
          {notification.message && (
            <div className="text-xs text-[var(--g5-text1)] mt-0.5 line-clamp-2">
              {notification.message}
            </div>
          )}
          <div className="text-[10px] text-[var(--g5-text2)] mt-1">
            {relativeTime(notification.createdAt)}
          </div>

          {/* Action buttons */}
          {notification.actions && notification.actions.length > 0 && (
            <div className="flex items-center gap-2 mt-2">
              {notification.actions.map((action, idx) => (
                <Button
                  key={idx}
                  kind="ghost"
                  onClick={() => onAction(notification.id, action)}
                >
                  {action.label}
                </Button>
              ))}
            </div>
          )}
        </div>

        {/* Dismiss button */}
        <button
          type="button"
          data-testid="notification-toast-dismiss-button"
          className={classNames(
            "shrink-0 p-0.5",
            "text-[var(--g5-text2)]",
            "hover:text-[var(--g5-text0)]",
            "transition-colors duration-150",
            "cursor-pointer",
          )}
          onClick={() => onDismiss(notification.id)}
          aria-label="Dismiss notification"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// NotificationToastStack Component
// ---------------------------------------------------------------------------

export const NotificationToastStack: React.FC<NotificationToastStackProps> = ({
  notifications,
  onDismiss,
  onAction,
}) => {
  // Track entering/exiting states for animation
  const [enteringIds, setEnteringIds] = useState<Set<string>>(new Set());
  const [exitingIds, setExitingIds] = useState<Set<string>>(new Set());
  const prevIdsRef = useRef<Set<string>>(new Set());
  const autoDismissTimers = useRef<Map<string, ReturnType<typeof setTimeout>>>(
    new Map(),
  );

  // Determine visible notifications (max 5)
  const visibleNotifications = notifications.slice(0, MAX_VISIBLE);

  // ---- Handle enter animations ----
  useEffect(() => {
    const currentIds = new Set(notifications.map((n) => n.id));
    const newIds = new Set<string>();

    currentIds.forEach((id) => {
      if (!prevIdsRef.current.has(id)) {
        newIds.add(id);
      }
    });

    if (newIds.size > 0) {
      setEnteringIds(newIds);
      // Remove entering state after animation completes
      const timer = setTimeout(() => {
        setEnteringIds(new Set());
      }, 50); // Small delay to trigger CSS transition
      return () => clearTimeout(timer);
    }

    prevIdsRef.current = currentIds;
  }, [notifications]);

  // ---- Auto-dismiss info notifications ----
  useEffect(() => {
    visibleNotifications.forEach((n) => {
      if (n.severity === "info" && !autoDismissTimers.current.has(n.id)) {
        const timer = setTimeout(
          () => {
            // Trigger exit animation
            setExitingIds((prev) => new Set([...prev, n.id]));
            // Actually dismiss after animation
            setTimeout(() => {
              onDismiss(n.id);
              setExitingIds((prev) => {
                const next = new Set(prev);
                next.delete(n.id);
                return next;
              });
            }, EXIT_DURATION_MS);
            autoDismissTimers.current.delete(n.id);
            // A toast with an action stays long enough to take it.
          },
          n.actions.length > 0 ? ACTION_TOAST_LIFETIME_MS : AUTO_DISMISS_MS,
        );
        autoDismissTimers.current.set(n.id, timer);
      }
    });

    // Cleanup timers for removed notifications
    return () => {
      const currentIds = new Set(notifications.map((n) => n.id));
      autoDismissTimers.current.forEach((timer, id) => {
        if (!currentIds.has(id)) {
          clearTimeout(timer);
          autoDismissTimers.current.delete(id);
        }
      });
    };
  }, [visibleNotifications, notifications, onDismiss]);

  // ---- Cleanup all timers on unmount ----
  useEffect(() => {
    return () => {
      autoDismissTimers.current.forEach((timer) => clearTimeout(timer));
      autoDismissTimers.current.clear();
    };
  }, []);

  // ---- Handle manual dismiss with animation ----
  const handleDismiss = useCallback(
    (id: string) => {
      // Clear auto-dismiss timer if any
      const timer = autoDismissTimers.current.get(id);
      if (timer) {
        clearTimeout(timer);
        autoDismissTimers.current.delete(id);
      }

      // Trigger exit animation
      setExitingIds((prev) => new Set([...prev, id]));

      // Actually dismiss after animation
      setTimeout(() => {
        onDismiss(id);
        setExitingIds((prev) => {
          const next = new Set(prev);
          next.delete(id);
          return next;
        });
      }, EXIT_DURATION_MS);
    },
    [onDismiss],
  );

  // Don't render anything if no notifications
  if (visibleNotifications.length === 0) return null;

  // Find or create portal root
  const portalRoot = document.getElementById("toast-portal") || document.body;

  return createPortal(
    <div
      className="fixed bottom-4 right-4 z-[60] flex flex-col-reverse gap-2"
      aria-label="Notifications"
      role="region"
    >
      {visibleNotifications.map((notification) => (
        <Toast
          key={notification.id}
          notification={notification}
          onDismiss={handleDismiss}
          onAction={onAction}
          entering={enteringIds.has(notification.id)}
          exiting={exitingIds.has(notification.id)}
        />
      ))}
    </div>,
    portalRoot,
  );
};
