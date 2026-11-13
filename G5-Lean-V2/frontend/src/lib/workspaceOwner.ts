// =============================================================================
// G5 Client — Workspace owner presentation
// =============================================================================
// How a member is named and how the owner row reads, shared by the phone
// lifecycle control's `WorkspaceVmStatusStrip` and the ribbon's
// `WorkbenchWorkspaceControls`: display name, else email, else the raw id,
// else "Unassigned".
// =============================================================================

import type { OrgMemberDTO } from "./types";

export function memberLabel(member: OrgMemberDTO): string {
  return member.displayName || member.email;
}

export function findOwner(
  members: OrgMemberDTO[],
  ownerUserId: string | null,
): OrgMemberDTO | undefined {
  return members.find((member) => member.userId === ownerUserId);
}

export function ownerName(
  members: OrgMemberDTO[],
  ownerUserId: string | null,
): string {
  const owner = findOwner(members, ownerUserId);
  return (owner && memberLabel(owner)) || ownerUserId || "Unassigned";
}

/** Everyone the owner could hand the workspace to. */
export function transferCandidates(
  members: OrgMemberDTO[],
  ownerUserId: string | null,
): OrgMemberDTO[] {
  return members.filter(
    (member) =>
      member.userId !== ownerUserId &&
      !member.deactivatedAt &&
      member.role !== "viewer",
  );
}
