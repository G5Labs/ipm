import { useCurrentUser } from "../../hooks/useAuth";
import { useOrgMembers } from "../../hooks/useOrg";
import type { WorkspacePullRequestAutoMergeRequest } from "../../lib/types";

const switchClass =
  "relative mt-0.5 h-4 w-7 shrink-0 cursor-pointer appearance-none rounded-full bg-[var(--g5-stroke)] transition-colors after:absolute after:left-0.5 after:top-0.5 after:h-3 after:w-3 after:rounded-full after:bg-white after:transition-transform checked:bg-[var(--g5-info)] checked:after:translate-x-3 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--g5-info)] disabled:cursor-not-allowed disabled:opacity-50";

export interface MergeWhenReadySwitchProps {
  orgId: string;
  /** The pull request's decision; null while merge when ready is off. */
  request: WorkspacePullRequestAutoMergeRequest | null;
  disabled?: boolean;
  onChange: (enabled: boolean) => void;
}

/**
 * The merge when ready switch of Prepare to Ship (g5.workbench.ship-to-parent,
 * g5.git.pr-auto-merge): on, G5 merges the pull request as the person who
 * turned it on once checks pass and approvals are met; no agent is started.
 * While on it names that person and why it is still waiting.
 */
export function MergeWhenReadySwitch({
  orgId,
  request,
  disabled = false,
  onChange,
}: MergeWhenReadySwitchProps) {
  const currentUser = useCurrentUser();
  const members = useOrgMembers(orgId);
  const author = !request
    ? null
    : currentUser.status === "ok" && request.enabledBy === currentUser.data.id
      ? "you"
      : ((members.status === "ok"
          ? members.data.find((member) => member.userId === request.enabledBy)
              ?.displayName
          : undefined) ?? "another member");

  return (
    <div
      data-testid="prepare-to-ship-auto-merge"
      className="flex items-start justify-between gap-4 p-3"
    >
      <div>
        <label
          htmlFor="prepare-to-ship-auto-merge-switch"
          className="text-xs font-medium text-[var(--g5-text1)]"
        >
          Merge when ready
        </label>
        <p
          id="prepare-to-ship-auto-merge-status"
          data-testid="prepare-to-ship-auto-merge-status"
          className="mt-1 text-[10.5px] leading-relaxed text-[var(--g5-text2)]"
        >
          {request
            ? `On, turned on by ${author}. G5 merges as ${author === "you" ? "you" : "them"} once checks pass and approvals are met.${request.reason ? ` ${request.reason}` : ""}`
            : "G5 merges it as you once checks pass and approvals are met. No agent is started."}
        </p>
      </div>
      <input
        type="checkbox"
        role="switch"
        id="prepare-to-ship-auto-merge-switch"
        data-testid="prepare-to-ship-auto-merge-switch"
        aria-describedby="prepare-to-ship-auto-merge-status"
        checked={request != null}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        className={switchClass}
      />
    </div>
  );
}

/** The source owner's independent choice shared by all shipping paths. */
export function StopVmAfterMergeSwitch({
  checked,
  disabled,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  onChange: (selected: boolean) => void;
}) {
  return (
    <div
      className={`flex items-center justify-between gap-4 border-b border-[var(--g5-stroke)] p-3 ${checked ? "bg-[var(--g5-info)]/10" : "bg-[var(--g5-bg0)]"}`}
    >
      <div>
        <label
          htmlFor="prepare-to-ship-stop-vm-switch"
          className="text-xs font-medium text-[var(--g5-text1)]"
        >
          Stop VM after merge
        </label>
        <p
          id="prepare-to-ship-stop-vm-help"
          className="mt-1 text-[10.5px] leading-relaxed text-[var(--g5-text2)]"
        >
          Applies to all merge options below.
        </p>
      </div>
      <input
        id="prepare-to-ship-stop-vm-switch"
        type="checkbox"
        role="switch"
        aria-describedby="prepare-to-ship-stop-vm-help"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        className={switchClass}
      />
    </div>
  );
}
