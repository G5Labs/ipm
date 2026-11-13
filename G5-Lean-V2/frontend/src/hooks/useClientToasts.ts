// =============================================================================
// G5 Client — Client-Side Toast Notifications
// =============================================================================
// Lightweight ephemeral toast system for displaying client-side errors,
// warnings, and info messages that originate from UI actions (e.g. failed
// mutations, import failures) rather than from the server notification API.
//
// Toasts auto-dismiss after a severity-dependent interval and are rendered
// via the existing NotificationToastStack component.
// =============================================================================

import { useCallback, useEffect, useRef, useState } from "react";
import type { G5Notification, NotificationAction } from "../lib/types";

/** Auto-dismiss delays by severity (ms). */
const DISMISS_DELAYS: Record<G5Notification["severity"], number> = {
  info: 4_000,
  warning: 6_000,
  error: 8_000,
};

/** A toast offering an action stays long enough to take it. */
export const ACTION_TOAST_LIFETIME_MS = 12_000;

/** How long a toast stays: longer when it carries an action. */
export function toastLifetimeMs(
  severity: G5Notification["severity"],
  actions: readonly NotificationAction[],
): number {
  return actions.length > 0
    ? Math.max(ACTION_TOAST_LIFETIME_MS, DISMISS_DELAYS[severity])
    : DISMISS_DELAYS[severity];
}

let nextToastId = 1;

/**
 * Hook providing an imperative `pushToast` function and the current list of
 * active toasts shaped as `G5Notification[]` for direct consumption by
 * `NotificationToastStack`.
 */
export function useClientToasts() {
  const [toasts, setToasts] = useState<G5Notification[]>([]);
  const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(
    new Map(),
  );

  // Auto-dismiss timers must not outlive the hook: a timer firing after
  // unmount sets state on a dead root (a CI-only "window is not defined"
  // unhandled error once the test environment is torn down).
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const timer of timers.values()) clearTimeout(timer);
      timers.clear();
    };
  }, []);

  const dismiss = useCallback((id: string) => {
    const timer = timersRef.current.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const pushToast = useCallback(
    (
      severity: G5Notification["severity"],
      title: string,
      message: string,
      actions: NotificationAction[] = [],
    ) => {
      const id = `client-toast-${nextToastId++}`;
      const notification: G5Notification = {
        id,
        workspaceId: null,
        severity,
        category: "agent_error", // reuse existing category; not displayed
        title,
        message,
        taskId: null,
        commentId: null,
        replyId: null,
        actions,
        createdAt: new Date().toISOString(),
        expiresAt: null,
        readAt: null,
      };

      setToasts((prev) => [...prev, notification]);

      // Schedule auto-dismiss
      const timer = setTimeout(
        () => {
          timersRef.current.delete(id);
          setToasts((prev) => prev.filter((t) => t.id !== id));
        },
        toastLifetimeMs(severity, actions),
      );
      timersRef.current.set(id, timer);

      return id;
    },
    [],
  );

  return { toasts, pushToast, dismiss } as const;
}
