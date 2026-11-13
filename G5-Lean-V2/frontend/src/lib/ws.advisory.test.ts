/**
 * Vitest unit test for the `workspace.remote_tip_advisory_updated` WS dispatch
 * (G4 / C1, design correction C17).
 *
 * The advisory is pure UI metadata — a BINARY "remote moved / behind" signal,
 * not a commit count — and it never moves the workspace tip. The WS handler
 * MUST therefore invalidate ONLY the branch sync-runs feed (which carries the
 * advisory SHA) and the workspace branch list, and MUST NOT invalidate
 * file-tree / ontology / node / live workspace file-draft caches (arch D21 / D11).
 *
 * SOT:
 * - views/arch_views/remote_repo_tracking.arch_view.md D21 (+ D11)
 * - aux_files/future_implementation/remote_tracking_gap_closure.plan.md G4
 * - Chat `remote-tracking-gap-closure-plan-2026-05-28`: C1, C17
 */

import { describe, it, expect, vi } from "vitest";
import type { QueryClient } from "@tanstack/react-query";
import { WebSocketManager, type WSEvent } from "./ws";

const WORKSPACE_ID = "ws-advisory";

function makeManager() {
  const invalidateQueries = vi.fn();
  const queryClient = { invalidateQueries } as unknown as QueryClient;
  const manager = new WebSocketManager({
    getBaseUrl: () => "ws://test",
    getWsTicket: () => Promise.resolve("ticket"),
    queryClient,
  });
  return { manager, invalidateQueries };
}

/** Collect the leading string token of every invalidated query key. */
function invalidatedKeyHeads(
  invalidateQueries: ReturnType<typeof vi.fn>,
): string[] {
  return invalidateQueries.mock.calls
    .map((call) => (call[0] as { queryKey?: unknown[] })?.queryKey?.[0])
    .filter((head): head is string => typeof head === "string");
}

describe("workspace.remote_tip_advisory_updated dispatch", () => {
  const event: WSEvent = {
    type: "workspace.remote_tip_advisory_updated",
    workspaceId: WORKSPACE_ID,
    repoId: "wbr-1",
    remoteTipAdvisorySha: "feedface",
  };

  it("invalidates ONLY the sync-runs feed and the workspace branch list", () => {
    const { manager, invalidateQueries } = makeManager();

    // handleEvent is the internal dispatch the socket message handler calls.
    (manager as unknown as { handleEvent: (e: WSEvent) => void }).handleEvent(
      event,
    );

    const heads = invalidatedKeyHeads(invalidateQueries);
    expect(heads).toContain("branch-sync-runs");
    expect(heads).toContain("workspace");

    // The advisory is pure UI metadata — it must NOT invalidate file/ontology
    // /node caches (those re-fetches would be a needless storm and the
    // advisory does not move the workspace tip).
    for (const forbidden of ["file-tree", "file-content", "docs"]) {
      expect(heads).not.toContain(forbidden);
    }

    // Scope check: the sync-runs invalidation is keyed to the event's workspace.
    const syncRunsCall = invalidateQueries.mock.calls.find(
      (c) =>
        (c[0] as { queryKey?: unknown[] })?.queryKey?.[0] ===
        "branch-sync-runs",
    );
    expect((syncRunsCall?.[0] as { queryKey: unknown[] }).queryKey).toEqual([
      "branch-sync-runs",
      WORKSPACE_ID,
    ]);
  });
});
