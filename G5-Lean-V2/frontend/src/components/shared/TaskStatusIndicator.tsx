import {
  AlertCircle,
  Ban,
  CheckCircle,
  CircleDashed,
  Loader,
  MessageCircleQuestionMark,
} from "lucide-react";
import type { TaskStatus } from "../../lib/types";

export const taskStatusTitle = (status: TaskStatus): string =>
  ({
    queued: "Queued",
    running: "Running",
    waiting: "Waiting to continue",
    succeeded: "Succeeded",
    failed: "Failed",
    cancelled: "Cancelled",
  })[status];

export function TaskStatusIndicator({
  status,
  size = 12,
}: {
  status: TaskStatus;
  size?: number;
}) {
  const Icon =
    {
      queued: CircleDashed,
      running: Loader,
      waiting: MessageCircleQuestionMark,
      succeeded: CheckCircle,
      failed: AlertCircle,
      cancelled: Ban,
    }[status] ?? CircleDashed;
  return (
    <Icon
      size={size}
      aria-hidden="true"
      data-testid="task-status-indicator"
      data-status-kind={status}
      className={
        status === "running"
          ? "animate-spin text-[var(--g5-info)]"
          : status === "succeeded"
            ? "text-[var(--g5-ok)]"
            : status === "waiting"
              ? "text-[var(--g5-warn)]"
              : "text-[var(--g5-text2)]"
      }
    />
  );
}
