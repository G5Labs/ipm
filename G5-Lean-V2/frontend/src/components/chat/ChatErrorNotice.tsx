import type { HTMLAttributes } from "react";
import { AlertCircle } from "lucide-react";
import { classNames } from "../ui/UI";

/** Shared by saved, live and journaled chat failures. The icon identifies the
 * error independently of color; the diagnostic is the accessible alert text. */
export function ChatErrorNotice({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      {...props}
      role="alert"
      className={classNames(
        "flex max-w-full items-start gap-1.5 rounded bg-[var(--g5-blocked)]/10 px-2 py-1 text-xs text-[var(--g5-blocked)]",
        className,
      )}
    >
      <AlertCircle size={14} aria-hidden="true" className="shrink-0" />
      <div className="min-w-0 whitespace-pre-wrap break-words">{children}</div>
    </div>
  );
}
