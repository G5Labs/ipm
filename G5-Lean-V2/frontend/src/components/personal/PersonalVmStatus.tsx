// The personal workspace's VM status chip, shared by Home's side panel and
// its narrow-screen panel (personal_workspace g5.personal.home).

import { VM_STATUS_LABEL } from "../../lib/workspaceVmControls";
import { classNames } from "../ui/UI";

export function PersonalVmStatus({
  status,
  testId,
}: {
  status: string;
  testId: string;
}) {
  return (
    <span
      data-testid={testId}
      className={classNames(
        "rounded-full px-2 py-0.5 text-[11px] font-medium",
        status === "running"
          ? "bg-[var(--g5-ok)]/15 text-[var(--g5-ok)]"
          : status === "failed"
            ? "bg-[var(--g5-blocked)]/15 text-[var(--g5-blocked)]"
            : "bg-[var(--g5-bg2)] text-[var(--g5-text1)]",
      )}
    >
      VM: {VM_STATUS_LABEL[status] ?? status}
    </span>
  );
}
