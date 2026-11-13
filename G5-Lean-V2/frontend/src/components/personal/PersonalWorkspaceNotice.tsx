// =============================================================================
// G5 Client — Personal workspace notice
// =============================================================================
// The personal workbench's one-line identity notice (personal_workspace
// g5.personal.access, g5.personal.workbench): it tells the owner that
// organization administrators can see their personal work and tells an
// administrator whose workspace they are reading.
// =============================================================================

import { Eye, UserRound } from "lucide-react";

export interface PersonalWorkspaceNoticeProps {
  /** The viewer owns this personal workspace. */
  isOwner: boolean;
  /** The owner's display name, for an administrator's view. */
  ownerName?: string | null;
}

export function PersonalWorkspaceNotice({
  isOwner,
  ownerName,
}: PersonalWorkspaceNoticeProps) {
  return (
    <div
      role="note"
      data-testid="personal-workspace-notice"
      className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-4 py-1.5 text-xs text-[var(--g5-text1)]"
    >
      {isOwner ? (
        <span className="inline-flex items-center gap-1.5">
          <UserRound size={13} className="text-[var(--g5-info)]" aria-hidden />
          <span>
            <span className="font-medium text-[var(--g5-text0)]">
              Your personal workspace.
            </span>{" "}
            Organization administrators can see your personal work.
          </span>
        </span>
      ) : (
        <span className="inline-flex items-center gap-1.5">
          <Eye size={13} className="text-[var(--g5-text2)]" aria-hidden />
          <span>
            <span className="font-medium text-[var(--g5-text0)]">
              {ownerName
                ? `${ownerName}'s personal workspace.`
                : "A member's personal workspace."}
            </span>{" "}
            You can read it as an organization administrator; only its owner can
            change it.
          </span>
        </span>
      )}
    </div>
  );
}
