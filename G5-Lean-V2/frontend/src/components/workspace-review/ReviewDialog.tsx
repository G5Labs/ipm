import type { ReactNode } from "react";
import { X } from "lucide-react";
import { Modal } from "../ui/UI";
export const reviewButton =
  "rounded border border-[var(--g5-stroke)] px-3 py-1.5 text-sm hover:bg-[var(--g5-bg2)] disabled:opacity-50";
export const reviewInput =
  "w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 text-sm";
export function ReviewDialog({
  title,
  onClose,
  children,
  busy = false,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  busy?: boolean;
}) {
  return (
    <Modal ariaLabel={title} size="lg" onClose={busy ? () => {} : onClose}>
      <div className="flex items-center justify-between p-5">
        <h2 className="text-lg font-semibold">{title}</h2>
        <button
          type="button"
          data-testid="review-dialog-close"
          aria-label="Close"
          disabled={busy}
          onClick={onClose}
          className={reviewButton}
        >
          <X size={16} />
        </button>
      </div>
      <div className="max-h-[70vh] overflow-auto px-5 pb-5">{children}</div>
    </Modal>
  );
}
