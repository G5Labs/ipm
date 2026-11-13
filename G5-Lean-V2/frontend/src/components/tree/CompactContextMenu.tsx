import type React from "react";
import { GatedControl } from "../../lib/workspaceLiveGate";
import { classNames } from "../ui/UI";

interface CompactContextMenuProps {
  x: number;
  y: number;
  label: string;
  children: React.ReactNode;
}

interface CompactContextMenuItemProps {
  icon: React.ReactNode;
  label: string;
  tone?: "normal" | "danger";
  onClick: () => void;
  /** Kept visible, disabled, with `title` saying why (`g5.workspace.ui.unavailable`). */
  disabled?: boolean;
  title?: string;
  /**
   * Why the item cannot act now; the item is disabled and, when tried,
   * reveals the reason and Start (`g5.ui.unavailable-actions`).
   */
  blockedReason?: string | null;
}

export function CompactContextMenu({
  x,
  y,
  label,
  children,
}: CompactContextMenuProps) {
  return (
    <div
      role="menu"
      aria-label={label}
      className={classNames(
        "fixed z-50 min-w-[112px] rounded border border-[var(--g5-stroke)]",
        "bg-[var(--g5-bg1)] shadow-md py-0.5",
      )}
      style={{ left: x, top: y }}
      onPointerDown={(event) => event.stopPropagation()}
    >
      {children}
    </div>
  );
}

export function CompactContextMenuItem({
  icon,
  label,
  tone = "normal",
  onClick,
  disabled = false,
  title,
  blockedReason = null,
}: CompactContextMenuItemProps) {
  const button = (
    <button
      type="button"
      data-testid="compact-context-menu-item"
      role="menuitem"
      disabled={disabled || blockedReason !== null}
      title={blockedReason ?? title}
      className={classNames(
        "w-full h-7 flex items-center gap-2 px-2 text-left text-xs",
        "hover:bg-[var(--g5-bg2)] transition-colors",
        "disabled:cursor-not-allowed disabled:opacity-50",
        tone === "danger"
          ? "text-[var(--g5-blocked)]"
          : "text-[var(--g5-text0)]",
      )}
      onClick={onClick}
    >
      <span className="flex h-3.5 w-3.5 items-center justify-center">
        {icon}
      </span>
      <span className="truncate">{label}</span>
    </button>
  );
  // A blocked item stays visible; trying it reveals why and, when Start is
  // the valid action, the header's Start (`g5.ui.unavailable-actions`).
  return (
    <GatedControl
      verb={label.toLowerCase()}
      reason={blockedReason}
      testId="compact-context-menu-item-hint"
      className="w-full"
    >
      {button}
    </GatedControl>
  );
}
