import { useEffect } from "react";
import { api } from "../lib/api";
import type { WorkspaceVmView } from "../lib/types";

/** Activity is evidence from the input event, independent of query/mutation
 * execution. Never send on mount, focus, reconnect, a timer or a refetch. */
export function observeWorkspaceUserActivity(
  target: Document,
  report: (
    reason: "pointer" | "keyboard" | "paste",
    observedAt: number,
  ) => void,
): () => void {
  let lastReport = -Infinity;
  const observe = (event: Event) => {
    if (!event.isTrusted || target.visibilityState === "hidden") return;
    if (
      event.target instanceof Element &&
      event.target.closest('[data-workspace-activity="ignore"]')
    )
      return;
    const now = performance.now();
    if (now - lastReport < 10_000) return;
    lastReport = now;
    report(
      event.type === "pointerdown" || event.type === "wheel"
        ? "pointer"
        : event.type === "paste"
          ? "paste"
          : "keyboard",
      now,
    );
  };
  const events = ["pointerdown", "keydown", "paste", "wheel"];
  for (const event of events) target.addEventListener(event, observe, true);
  return () => {
    for (const event of events)
      target.removeEventListener(event, observe, true);
  };
}

export function useWorkspaceUserActivity(
  workspaceId: string | undefined,
  vm: WorkspaceVmView | null,
  isOwner: boolean,
) {
  const vmId = vm?.vmId;
  const epoch = vm?.epoch;
  const running = vm?.status === "running";
  useEffect(() => {
    if (!workspaceId || !isOwner || !running || !vmId) return;
    const abort = new AbortController();
    const remove = observeWorkspaceUserActivity(
      document,
      (reason, gestureTime) => {
        const gestureWallTime = Date.now();
        const path = `/api/v1/workspaces/${encodeURIComponent(workspaceId)}/user-activity`;
        // Sample platform time only for this gesture. The passive GET may
        // recover authentication, but all elapsed time still ages the event.
        void api
          .get<{ serverTime: string }>(path, { signal: abort.signal })
          .then(({ serverTime }) => {
            // Some browsers pause their monotonic clock during OS sleep. A
            // wall-clock *difference* can only age/drop a report, never date
            // it; a fixed client clock skew cancels out.
            const elapsed = Math.max(
              performance.now() - gestureTime,
              Date.now() - gestureWallTime,
            );
            const platformTime = Date.parse(serverTime);
            if (
              abort.signal.aborted ||
              elapsed < 0 ||
              elapsed > 30_000 ||
              !Number.isFinite(platformTime)
            )
              return;
            // Authentication recovery can replay only this immutable body,
            // never re-date the event or move it to another incarnation.
            return api.post(
              path,
              {
                vmId,
                epoch,
                reason,
                observedAt: new Date(platformTime - elapsed).toISOString(),
              },
              { signal: abort.signal },
            );
          })
          .catch(() => {});
      },
    );
    return () => {
      remove();
      abort.abort();
    };
  }, [workspaceId, isOwner, running, vmId, epoch]);
}
