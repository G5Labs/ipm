// =============================================================================
// Approvals requested from the signed-in person
// (g5.governance.approval.personal-requests)
// =============================================================================
// Presentation only: the server's per-requirement `callerCanDecide` already
// applies the addressed active-request, request-PR and self-approval rules,
// independently of the requirement's status, so a requirement met by someone
// else still lists a personal request that remains actionable.
// =============================================================================

import type { RefreshingState } from "../hooks/hookUtils";
import type { WorkspaceApprovalsStateDTO } from "./types";

export type PersonalApprovals =
  | { kind: "loading" }
  | { kind: "unavailable" }
  | {
      kind: "requested";
      /** Requirement names, alphabetical, one per actionable requirement. */
      names: string[];
      /** The one person who requested every listed review, when known. */
      requester: string | null;
    }
  | { kind: "none" };

export function personalApprovals(
  state: RefreshingState<WorkspaceApprovalsStateDTO>,
  viewerUserId: string,
): PersonalApprovals {
  if (state.status === "loading") return { kind: "loading" };
  // A failed refresh may hide a newer request: never read it as zero.
  if (state.status === "error" || state.refreshError)
    return { kind: "unavailable" };
  const requirements = (state.data.outgoing?.requirements ?? [])
    .filter((requirement) => requirement.callerCanDecide)
    .sort((a, b) => a.definition.name.localeCompare(b.definition.name));
  if (requirements.length === 0) return { kind: "none" };
  const requesters = new Set(
    requirements.flatMap((requirement) =>
      (requirement.requests ?? [])
        .filter(
          (request) =>
            request.state === "pending" &&
            request.actorType === "human" &&
            request.actorUserId === viewerUserId,
        )
        .map((request) => request.requestedBy?.displayName ?? null),
    ),
  );
  const [only] = [...requesters];
  return {
    kind: "requested",
    names: requirements.map((requirement) => requirement.definition.name),
    requester: requesters.size === 1 && only ? only : null,
  };
}

export function personalApprovalsTitle(count: number): string {
  return count === 1
    ? "1 approval requested from you"
    : `${count} approvals requested from you`;
}
