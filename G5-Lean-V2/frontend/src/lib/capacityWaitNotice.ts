// Which capacity waits this page load has already told the owner about, so a
// wait is announced once per Start however long it lasts or however often the
// owner moves around the app and back, and a later Start afresh
// (`g5.ui.capacity-wait-notice`).

import type { WorkspaceVmView } from "./types";

type WaitIdentity = Pick<WorkspaceVmView, "workspaceId" | "startIntent">;

const announced = new Set<string>();

/**
 * One key per Start: the start intent's id, which outlives placement retries
 * and polls. A projection without an id (never the current API) keys on the
 * workspace alone, so its waits are told apart only by being seen to end.
 */
export function capacityWaitKey(vm: WaitIdentity): string {
  const intentId = vm.startIntent?.id;
  return `${vm.workspaceId}:${typeof intentId === "string" ? intentId : ""}`;
}

/** Records the announcement; false when this wait was already announced. */
export function markCapacityWaitAnnounced(key: string): boolean {
  if (announced.has(key)) return false;
  announced.add(key);
  return true;
}

/** Takes back an announcement that was not shown after all. */
export function forgetCapacityWaitAnnouncement(key: string): void {
  announced.delete(key);
}

/**
 * The workspace is seen not waiting: its earlier Starts are forgotten. The
 * current Start's own key stays, so a placement that bounces back to the
 * queue is not announced a second time.
 */
export function forgetEndedCapacityWaits(vm: WaitIdentity): void {
  const current =
    typeof vm.startIntent?.id === "string" ? capacityWaitKey(vm) : null;
  for (const key of announced) {
    if (key.startsWith(`${vm.workspaceId}:`) && key !== current) {
      announced.delete(key);
    }
  }
}

/** Tests share the module; each starts from a page that announced nothing. */
export function resetCapacityWaitAnnouncementsForTests(): void {
  announced.clear();
}
