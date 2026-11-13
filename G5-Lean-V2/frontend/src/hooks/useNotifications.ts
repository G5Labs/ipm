// =============================================================================
// G5 Client — Notification State Management
// =============================================================================
// Hydrates notifications from REST API on mount, subscribes to real-time
// updates via WebSocket 'notification' events on the user:<uuid> channel,
// and tracks read/dismissed state with server-side persistence.
// =============================================================================

import { useState, useCallback, useRef, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import type { G5Notification } from "../lib/types";
import { useWSChannel, useWSEventListener } from "./useWebSocket";
import { api } from "../lib/api";
import { isUuid } from "../lib/utils";

/** Auto-expiry delay for info notifications (5 seconds) */
const INFO_EXPIRY_MS = 5_000;

/** Maximum number of notifications to keep in memory */
const MAX_NOTIFICATIONS = 50;

function isCommentOrMentionNotification(notification: G5Notification): boolean {
  return Boolean(
    notification.category === "mention" ||
    notification.category === "comment" ||
    notification.commentId ||
    notification.replyId,
  );
}

/**
 * Whether a notification is a passing toast: an `info` notice that is not
 * inbox material. Comment and mention notifications, review activity (a
 * pushed round, a decision, a reply) and pull request announcements
 * (g5.git.pr-merge-announcement) wait for the reader and never time out.
 */
function autoExpires(notification: G5Notification): boolean {
  if (isCommentOrMentionNotification(notification)) return false;
  if (notification.category === "review") return false;
  if (notification.category === "pull_request") return false;
  if (notification.severity !== "info") return false;
  return Boolean(notification.createdAt);
}

/**
 * Check whether a notification has expired: a passing toast older than
 * INFO_EXPIRY_MS. Everything else is sticky until dismissed or read.
 */
function isExpired(notification: G5Notification): boolean {
  if (!autoExpires(notification)) return false;
  const age = Date.now() - new Date(notification.createdAt).getTime();
  return age > INFO_EXPIRY_MS;
}

/**
 * Derive the highest severity from a list of active notifications.
 * Priority: error > warning > info > null
 */
function deriveHighestSeverity(
  active: G5Notification[],
): "info" | "warning" | "error" | null {
  if (active.length === 0) return null;
  if (active.some((n) => n.severity === "error")) return "error";
  if (active.some((n) => n.severity === "warning")) return "warning";
  return "info";
}

/** REST API response shape for a notification */
interface NotificationApiDTO {
  id: string;
  workspaceId: string;
  category: string;
  severity: string;
  title: string;
  message: string;
  taskId?: string | null;
  commentId?: string | null;
  replyId?: string | null;
  actions: Array<{ label: string; action: string; target: string }>;
  readAt?: string | null;
  expiresAt?: string | null;
  createdAt: string;
  actorName?: string | null;
  actorAvatar?: string | null;
}

/** Convert REST DTO to G5Notification (the list is scoped to `workspaceId`) */
function toNotification(
  dto: NotificationApiDTO,
  workspaceId: string,
): G5Notification {
  return {
    id: dto.id,
    workspaceId: dto.workspaceId || workspaceId,
    severity: dto.severity as "info" | "warning" | "error",
    category: dto.category as G5Notification["category"],
    title: dto.title,
    message: dto.message,
    taskId: dto.taskId ?? null,
    commentId: dto.commentId ?? null,
    replyId: dto.replyId ?? null,
    actions: (dto.actions ?? []).map((a) => ({
      label: a.label,
      action: a.action as "navigate" | "open_chat",
      target: a.target,
    })),
    createdAt: dto.createdAt,
    expiresAt: dto.expiresAt ?? null,
    readAt: dto.readAt ?? null,
  };
}

export function useNotifications(
  workspaceId: string,
  userId?: string,
): {
  notifications: G5Notification[];
  active: G5Notification[];
  dismiss: (id: string) => void;
  markRead: (id: string) => void;
  /**
   * Clear all: dismiss every active notification now and mark it read on the
   * server, so the inbox stays cleared after a reload (g5.user.notifications).
   */
  clearAll: () => void;
  highestSeverity: "info" | "warning" | "error" | null;
  isLoading: boolean;
} {
  const queryClient = useQueryClient();

  // Subscribe to UUID-backed channels only. During workspace slug resolution
  // these values can temporarily be empty/slugs; sending those to the API causes
  // INVALID_CHANNEL noise and reconnect loops.
  useWSChannel(isUuid(workspaceId) ? `workspace:${workspaceId}` : "");
  useWSChannel(isUuid(userId) ? `user:${userId}` : "");

  // ---------------------------------------------------------------------------
  // REST hydration — fetch existing notifications on mount
  // ---------------------------------------------------------------------------

  const { data: serverNotifications, isLoading } = useQuery<G5Notification[]>({
    queryKey: ["notifications", workspaceId],
    queryFn: async () => {
      const dtos = await api.get<NotificationApiDTO[]>(
        `/api/v1/workspaces/${workspaceId}/notifications`,
      );
      return dtos.map((dto) => toNotification(dto, workspaceId));
    },
    enabled: !!workspaceId,
    staleTime: 30_000,
  });

  const [localNotifications, setLocalNotifications] = useState<
    G5Notification[]
  >([]);
  const dismissedRef = useRef<Set<string>>(new Set());
  const seenIdsRef = useRef<Set<string>>(new Set());

  // Merge server notifications with local real-time ones
  const notifications = (() => {
    const merged = [...localNotifications];
    const localIds = new Set(localNotifications.map((n) => n.id));
    for (const n of serverNotifications ?? []) {
      if (!localIds.has(n.id)) {
        merged.push(n);
      }
    }
    return merged.slice(0, MAX_NOTIFICATIONS);
  })();

  // What the bell counts and the panel lists: one unread set, so a dismissal
  // or Clear all empties both at once.
  const active = notifications.filter(
    (n) => !dismissedRef.current.has(n.id) && !n.readAt && !isExpired(n),
  );

  // ---------------------------------------------------------------------------
  // Real-time WS updates — push new notifications
  // ---------------------------------------------------------------------------

  useWSEventListener("notification", (event: Record<string, unknown>) => {
    const payload = event.notification as Record<string, unknown> | undefined;
    if (!payload || typeof payload !== "object") return;

    const id = payload.id as string;
    if (!id || seenIdsRef.current.has(id)) return;
    seenIdsRef.current.add(id);

    const notification: G5Notification = {
      id,
      // The user channel delivers rows from every workspace the person
      // belongs to; keep the owning workspace so Clear all can mark the
      // row read where it lives.
      workspaceId: (event.workspaceId as string) || workspaceId,
      severity: ((payload.severity as string) ||
        "info") as G5Notification["severity"],
      category: ((payload.category as string) ||
        "mention") as G5Notification["category"],
      title: (payload.title as string) || "",
      message: (payload.message as string) || "",
      taskId: (payload.taskId as string) ?? null,
      commentId: (payload.commentId as string) ?? null,
      replyId: (payload.replyId as string) ?? null,
      actions: Array.isArray(payload.actions)
        ? (payload.actions as G5Notification["actions"])
        : [],
      createdAt: (payload.createdAt as string) || new Date().toISOString(),
      expiresAt: (payload.expiresAt as string) ?? null,
      readAt: (payload.readAt as string) ?? null,
    };

    setLocalNotifications((prev) => {
      const updated = [notification, ...prev];
      return updated.length > MAX_NOTIFICATIONS
        ? updated.slice(0, MAX_NOTIFICATIONS)
        : updated;
    });

    // Also invalidate the query to keep REST data fresh
    queryClient.invalidateQueries({ queryKey: ["notifications", workspaceId] });
  });

  // ---------------------------------------------------------------------------
  // Dismiss / Mark Read / ClearAll
  // ---------------------------------------------------------------------------

  const dismiss = useCallback((id: string) => {
    dismissedRef.current.add(id);
    setLocalNotifications((prev) => [...prev]);
  }, []);

  const markReadMutation = useMutation<void, Error, string>({
    mutationFn: (notificationId: string) =>
      api.patch<void>(`/api/v1/notifications/${notificationId}`),
    onSuccess: (_data, notificationId) => {
      dismissedRef.current.add(notificationId);
      setLocalNotifications((prev) => [...prev]);
      queryClient.invalidateQueries({
        queryKey: ["notifications", workspaceId],
      });
    },
  });

  const markRead = useCallback(
    (id: string) => {
      markReadMutation.mutate(id);
    },
    [markReadMutation],
  );

  // Persist a Clear all as read: exactly the rows that were shown, so a
  // notification that arrives after the click stays unread, grouped by the
  // workspace that stores each row (read-all is workspace-scoped, and the
  // user channel delivers rows from other workspaces too). Client-only rows
  // (no workspace) have nothing to persist.
  const clearMutation = useMutation<void, Error, G5Notification[]>({
    mutationFn: async (cleared) => {
      const byWorkspace = new Map<string, string[]>();
      for (const n of cleared) {
        if (!n.workspaceId) continue;
        const ids = byWorkspace.get(n.workspaceId) ?? [];
        ids.push(n.id);
        byWorkspace.set(n.workspaceId, ids);
      }
      const requests = [...byWorkspace].map(([owner, ids]) =>
        api.post<void>(`/api/v1/workspaces/${owner}/notifications/read-all`, {
          ids,
        }),
      );
      const results = await Promise.allSettled(requests);
      const failed = results.find(
        (r): r is PromiseRejectedResult => r.status === "rejected",
      );
      if (failed) throw failed.reason;
    },
    onSettled: (_data, _error, cleared) => {
      // Every inbox the clear touched refetches, not only the one on screen:
      // another workspace's cached list would otherwise show the cleared
      // rows again when it is opened within its stale window.
      const owners = new Set<string>([workspaceId]);
      for (const n of cleared) if (n.workspaceId) owners.add(n.workspaceId);
      for (const owner of owners) {
        queryClient.invalidateQueries({ queryKey: ["notifications", owner] });
      }
    },
  });

  const clearAll = useCallback(() => {
    if (active.length === 0) return;
    // Dismiss locally first so the panel and the bell empty at once; the
    // dismissal holds for the session even if persisting fails.
    active.forEach((n) => dismissedRef.current.add(n.id));
    setLocalNotifications((prev) => [...prev]);
    clearMutation.mutate(active);
  }, [active, clearMutation]);

  // ---------------------------------------------------------------------------
  // Auto-expiry for info notifications
  // ---------------------------------------------------------------------------

  useEffect(() => {
    // Only passing toasts get a timer: the same rule as `isExpired`, so the
    // panel, which lists `active`, never loses inbox material to it.
    const infoNotifications = notifications.filter(
      (n) =>
        autoExpires(n) && !dismissedRef.current.has(n.id) && !isExpired(n),
    );

    if (infoNotifications.length === 0) return;

    const timers = infoNotifications.map((n) => {
      const age = Date.now() - new Date(n.createdAt).getTime();
      const remaining = Math.max(0, INFO_EXPIRY_MS - age);
      return setTimeout(() => {
        dismissedRef.current.add(n.id);
        setLocalNotifications((prev) => [...prev]);
      }, remaining);
    });

    return () => {
      timers.forEach((t) => clearTimeout(t));
    };
  }, [notifications]);

  // ---------------------------------------------------------------------------
  // Derived state
  // ---------------------------------------------------------------------------

  const highestSeverity = deriveHighestSeverity(active);

  return {
    notifications,
    active,
    dismiss,
    markRead,
    clearAll,
    highestSeverity,
    isLoading,
  };
}
